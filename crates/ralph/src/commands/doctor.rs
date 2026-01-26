//! Doctor checks for Git, queue, and runner configuration health.

use crate::config;
use crate::contracts::Runner;
use crate::gitutil;
use crate::outpututil;
use crate::queue;
use crate::runner;
use anyhow::Result;
use std::fs;
use std::process::Command;

pub fn run_doctor(resolved: &config::Resolved) -> Result<()> {
    log::info!("Running doctor check...");
    let mut failures = Vec::new();

    // 1. Git Checks
    log::info!("Checking Git environment...");
    if let Err(e) = check_command("git", &["--version"]) {
        outpututil::log_error(&format!("git binary not found or not executable: {}", e));
        failures.push("git binary missing");
    } else {
        outpututil::log_success("git binary found");
    }

    match gitutil::status_porcelain(&resolved.repo_root) {
        Ok(_) => outpututil::log_success(&format!(
            "valid git repo at {}",
            resolved.repo_root.display()
        )),
        Err(e) => {
            outpututil::log_error(&format!("invalid git repo: {}", e));
            failures.push("invalid git repo");
        }
    }

    match gitutil::upstream_ref(&resolved.repo_root) {
        Ok(u) => outpututil::log_success(&format!("upstream configured: {}", u)),
        Err(e) => {
            outpututil::log_warn(&format!("no upstream configured: {}", e));
        }
    }

    // Git LFS Checks
    log::info!("Checking Git LFS...");
    match gitutil::has_lfs(&resolved.repo_root) {
        Ok(true) => {
            outpututil::log_success("Git LFS detected");
            match gitutil::list_lfs_files(&resolved.repo_root) {
                Ok(files) => {
                    if files.is_empty() {
                        log::info!("LFS initialized but no files tracked");
                    } else {
                        outpututil::log_success(&format!("LFS tracking {} file(s)", files.len()));
                    }
                }
                Err(e) => {
                    outpututil::log_warn(&format!("Failed to list LFS files: {}", e));
                }
            }
        }
        Ok(false) => {
            log::info!("Git LFS not detected");
        }
        Err(e) => {
            outpututil::log_warn(&format!("LFS check failed: {}", e));
        }
    }

    // 2. Queue Checks
    log::info!("Checking Ralph queue...");
    if resolved.queue_path.exists() {
        match queue::load_queue(&resolved.queue_path) {
            Ok(q) => match queue::validate_queue(&q, &resolved.id_prefix, resolved.id_width) {
                Ok(_) => outpututil::log_success(&format!("queue valid ({} tasks)", q.tasks.len())),
                Err(e) => {
                    outpututil::log_error(&format!("queue validation failed: {}", e));
                    failures.push("queue validation failed");
                }
            },
            Err(e) => {
                outpututil::log_error(&format!("failed to load queue: {}", e));
                failures.push("queue load failed");
            }
        }
    } else {
        outpututil::log_error(&format!(
            "queue file missing at {}",
            resolved.queue_path.display()
        ));
        failures.push("missing queue file");
    }

    // 2b. Done Archive Checks
    log::info!("Checking Ralph done archive...");
    if resolved.done_path.exists() {
        match queue::load_queue(&resolved.done_path) {
            Ok(d) => match queue::validate_queue(&d, &resolved.id_prefix, resolved.id_width) {
                Ok(_) => outpututil::log_success(&format!(
                    "done archive valid ({} tasks)",
                    d.tasks.len()
                )),
                Err(e) => {
                    outpututil::log_error(&format!("done archive validation failed: {}", e));
                    failures.push("done archive validation failed");
                }
            },
            Err(e) => {
                outpututil::log_error(&format!("failed to load done archive: {}", e));
                failures.push("done archive load failed");
            }
        }
    } else {
        log::info!("done archive missing (optional)");
    }

    // 3. Runner Checks
    log::info!("Checking Agent configuration...");
    let runner = resolved.config.agent.runner.unwrap_or_default();
    let runner_configured = runner_configured(resolved);
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
        Runner::Claude => resolved
            .config
            .agent
            .claude_bin
            .as_deref()
            .unwrap_or("claude"),
        Runner::Cursor => resolved
            .config
            .agent
            .cursor_bin
            .as_deref()
            .unwrap_or("agent"),
    };

    if let Err(e) = check_command(bin_name, &["--version"]) {
        let message = format!(
            "runner binary '{}' ({:?}) check failed: {}",
            bin_name, runner, e
        );
        if runner_configured {
            outpututil::log_error(&message);
            failures.push("runner binary missing");
        } else {
            outpututil::log_warn(&message);
        }
    } else {
        outpututil::log_success(&format!(
            "runner binary '{}' ({:?}) found",
            bin_name, runner
        ));
    }

    // 3b. Model Compatibility Check
    let model = runner::resolve_model_for_runner(
        runner,
        None,
        None,
        resolved.config.agent.model.clone(),
        false,
    );
    if let Err(e) = runner::validate_model_for_runner(runner, &model) {
        outpututil::log_error(&format!("config model/runner mismatch: {}", e));
        failures.push("config model/runner mismatch");
    } else {
        outpututil::log_success(&format!(
            "model '{}' compatible with runner '{:?}'",
            model.as_str(),
            runner
        ));
    }

    // 4. Project Checks
    log::info!("Checking project environment...");
    let makefile_path = resolved.repo_root.join("Makefile");
    if makefile_path.exists() {
        outpututil::log_success("Makefile found");
        match fs::read_to_string(&makefile_path) {
            Ok(content) => {
                if content.contains("ci:") {
                    outpututil::log_success("Makefile has 'ci' target");
                } else {
                    outpututil::log_warn("Makefile exists but missing 'ci' target");
                }
            }
            Err(e) => {
                outpututil::log_error(&format!("failed to read Makefile: {}", e));
                failures.push("failed to read Makefile");
            }
        }
    } else {
        outpututil::log_error("Makefile missing in repo root");
        failures.push("missing Makefile");
    }

    if failures.is_empty() {
        log::info!("Doctor check passed. System is ready.");
        Ok(())
    } else {
        outpututil::log_error(&format!("Doctor found {} issue(s):", failures.len()));
        for fail in &failures {
            log::error!("  - {}", fail);
        }
        anyhow::bail!("Doctor check failed: one or more critical components are missing or misconfigured. Review the error logs above and fix the reported issues before running Ralph.");
    }
}

fn runner_configured(resolved: &config::Resolved) -> bool {
    let mut configured = false;
    let mut consider_layer = |path: &std::path::Path| {
        if configured {
            return;
        }
        let layer = match config::load_layer(path) {
            Ok(layer) => layer,
            Err(err) => {
                log::warn!("Unable to load config layer at {}: {}", path.display(), err);
                return;
            }
        };
        configured = layer.agent.runner.is_some()
            || layer.agent.codex_bin.is_some()
            || layer.agent.opencode_bin.is_some()
            || layer.agent.gemini_bin.is_some()
            || layer.agent.claude_bin.is_some();
    };

    if let Some(path) = resolved.global_config_path.as_ref() {
        if path.exists() {
            consider_layer(path);
        }
    }
    if let Some(path) = resolved.project_config_path.as_ref() {
        if path.exists() {
            consider_layer(path);
        }
    }

    configured
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
