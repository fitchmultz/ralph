use crate::config;
use crate::contracts::Runner;
use crate::gitutil;
use crate::queue;
use crate::runner;
use anyhow::Result;
use std::fs;
use std::process::Command;

pub fn run_doctor(resolved: &config::Resolved) -> Result<()> {
    println!(">> [RALPH] Running doctor check...");
    let mut failures = Vec::new();

    // 1. Git Checks
    println!("Checking Git environment...");
    if let Err(e) = check_command("git", &["--version"]) {
        println!("  [FAIL] git binary not found or not executable: {}", e);
        failures.push("git binary missing");
    } else {
        println!("  [OK] git binary found");
    }

    match gitutil::status_porcelain(&resolved.repo_root) {
        Ok(_) => println!("  [OK] valid git repo at {}", resolved.repo_root.display()),
        Err(e) => {
            println!("  [FAIL] invalid git repo: {}", e);
            failures.push("invalid git repo");
        }
    }

    match gitutil::upstream_ref(&resolved.repo_root) {
        Ok(u) => println!("  [OK] upstream configured: {}", u),
        Err(e) => {
            println!("  [WARN] no upstream configured: {}", e);
        }
    }

    // 2. Queue Checks
    println!("Checking Ralph queue...");
    if resolved.queue_path.exists() {
        match queue::load_queue_with_repair(&resolved.queue_path) {
            Ok((q, repaired)) => {
                queue::warn_if_repaired(&resolved.queue_path, repaired);
                match queue::validate_queue(&q, &resolved.id_prefix, resolved.id_width) {
                    Ok(_) => println!("  [OK] queue valid ({} tasks)", q.tasks.len()),
                    Err(e) => {
                        println!("  [FAIL] queue validation failed: {}", e);
                        failures.push("queue validation failed");
                    }
                }
            }
            Err(e) => {
                println!("  [FAIL] failed to load queue: {}", e);
                failures.push("queue load failed");
            }
        }
    } else {
        println!(
            "  [FAIL] queue file missing at {}",
            resolved.queue_path.display()
        );
        failures.push("missing queue file");
    }

    // 2b. Done Archive Checks
    println!("Checking Ralph done archive...");
    if resolved.done_path.exists() {
        match queue::load_queue_with_repair(&resolved.done_path) {
            Ok((d, repaired)) => {
                queue::warn_if_repaired(&resolved.done_path, repaired);
                match queue::validate_queue(&d, &resolved.id_prefix, resolved.id_width) {
                    Ok(_) => println!("  [OK] done archive valid ({} tasks)", d.tasks.len()),
                    Err(e) => {
                        println!("  [FAIL] done archive validation failed: {}", e);
                        failures.push("done archive validation failed");
                    }
                }
            }
            Err(e) => {
                println!("  [FAIL] failed to load done archive: {}", e);
                failures.push("done archive load failed");
            }
        }
    } else {
        println!("  [OK] done archive missing (optional)");
    }

    // 3. Runner Checks
    println!("Checking Agent configuration...");
    let runner = resolved.config.agent.runner.unwrap_or_default();
    let bin_name = match runner {
        Runner::Codex => resolved
            .config
            .agent
            .codex_bin
            .as_deref()
            .unwrap_or("codex"),
        Runner::Opencode => resolved
            .config
            .agent
            .opencode_bin
            .as_deref()
            .unwrap_or("opencode"),
        Runner::Gemini => resolved
            .config
            .agent
            .gemini_bin
            .as_deref()
            .unwrap_or("gemini"),
    };

    if let Err(e) = check_command(bin_name, &["--version"]) {
        println!(
            "  [FAIL] runner binary '{}' ({:?}) check failed: {}",
            bin_name, runner, e
        );
        failures.push("runner binary missing");
    } else {
        println!("  [OK] runner binary '{}' ({:?}) found", bin_name, runner);
    }

    // 3b. Model Compatibility Check
    let model =
        runner::resolve_model_for_runner(runner, None, None, resolved.config.agent.model.clone());
    if let Err(e) = runner::validate_model_for_runner(runner, &model) {
        println!("  [FAIL] config model/runner mismatch: {}", e);
        failures.push("config model/runner mismatch");
    } else {
        println!(
            "  [OK] model '{}' compatible with runner '{:?}'",
            model.as_str(),
            runner
        );
    }

    // 4. Project Checks
    println!("Checking project environment...");
    let makefile_path = resolved.repo_root.join("Makefile");
    if makefile_path.exists() {
        println!("  [OK] Makefile found");
        match fs::read_to_string(&makefile_path) {
            Ok(content) => {
                if content.contains("ci:") {
                    println!("  [OK] Makefile has 'ci' target");
                } else {
                    println!("  [WARN] Makefile exists but missing 'ci' target");
                }
            }
            Err(e) => {
                println!("  [FAIL] failed to read Makefile: {}", e);
                failures.push("failed to read Makefile");
            }
        }
    } else {
        println!("  [FAIL] Makefile missing in repo root");
        failures.push("missing Makefile");
    }

    if failures.is_empty() {
        println!("\n>> [RALPH] Doctor check passed. System is ready.");
        Ok(())
    } else {
        eprintln!("\n>> [RALPH] Doctor found {} issue(s):", failures.len());
        for fail in &failures {
            eprintln!("  - {}", fail);
        }
        anyhow::bail!("doctor check failed");
    }
}

fn check_command(bin: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_msg = if stderr.trim().is_empty() {
            format!(
                "command '{}' {:?} failed with exit status: {}",
                bin, args, output.status
            )
        } else {
            format!(
                "command '{}' {:?} failed with exit status {}: {}",
                bin,
                args,
                output.status,
                stderr.trim()
            )
        };
        Err(anyhow::anyhow!(stderr_msg))
    }
}
