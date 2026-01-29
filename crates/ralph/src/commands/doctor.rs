//! Doctor checks for Git, queue, and runner configuration health.
//!
//! Responsibilities:
//! - Verify Git environment and repository health
//! - Validate queue and done archive files
//! - Check runner binary availability and configuration
//! - Detect orphaned lock directories that may accumulate over time
//!
//! Not handled here:
//! - Automatic repair of detected issues (doctor is read-only)
//! - Performance benchmarking or stress testing
//! - Network connectivity checks
//!
//! Invariants/assumptions:
//! - All checks are independent; failures in one don't prevent others
//! - Output uses outpututil for consistent formatting
//! - Returns Ok only when all critical checks pass

use crate::config;
use crate::contracts::Runner;
use crate::gitutil;
use crate::lock::{pid_is_running, queue_lock_dir};
use crate::outpututil;
use crate::prompts;
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

    // 2c. Lock Health Checks
    log::info!("Checking Ralph lock health...");
    match check_lock_health(&resolved.repo_root) {
        Ok((orphaned_count, total_count)) => {
            if orphaned_count > 0 {
                outpututil::log_warn(&format!(
                    "found {} orphaned lock director{} (out of {} total)",
                    orphaned_count,
                    if orphaned_count == 1 { "y" } else { "ies" },
                    total_count
                ));
                failures.push("orphaned lock directories detected");
            } else if total_count > 0 {
                outpututil::log_success(&format!(
                    "all {} lock director{} healthy",
                    total_count,
                    if total_count == 1 { "y" } else { "ies" }
                ));
            } else {
                log::info!("no lock directories found");
            }
        }
        Err(e) => {
            outpututil::log_warn(&format!("lock health check failed: {}", e));
        }
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
        Runner::Kimi => resolved.config.agent.kimi_bin.as_deref().unwrap_or("kimi"),
        Runner::Pi => resolved.config.agent.pi_bin.as_deref().unwrap_or("pi"),
    };

    if let Err(e) = check_runner_binary(bin_name) {
        let config_key = get_runner_config_key(runner);
        let message = format!(
            "runner binary '{}' ({:?}) check failed: {}",
            bin_name, runner, e
        );
        if runner_configured {
            outpututil::log_error(&message);
            log::error!("");
            log::error!("To fix this issue:");
            log::error!("  1. Install the runner binary, or");
            log::error!("  2. Configure a custom path in .ralph/config.json:");
            log::error!("     {{");
            log::error!("       \"agent\": {{");
            log::error!("         \"{}\": \"/path/to/{}\"", config_key, bin_name);
            log::error!("       }}");
            log::error!("     }}");
            log::error!("  3. Run 'ralph doctor' to verify the fix");
            failures.push("runner binary missing");
        } else {
            outpututil::log_warn(&message);
            log::warn!("");
            log::warn!("To fix this issue:");
            log::warn!("  1. Install the runner binary, or");
            log::warn!("  2. Configure a custom path in .ralph/config.json:");
            log::warn!("     {{");
            log::warn!("       \"agent\": {{");
            log::warn!("         \"{}\": \"/path/to/{}\"", config_key, bin_name);
            log::warn!("       }}");
            log::warn!("     }}");
            log::warn!("  3. Run 'ralph doctor' to verify the fix");
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

    // 3c. Instruction file injection checks
    log::info!("Checking instruction file injection...");
    let instruction_warnings =
        prompts::instruction_file_warnings(&resolved.repo_root, &resolved.config);
    if instruction_warnings.is_empty() {
        if let Some(files) = resolved.config.agent.instruction_files.as_ref() {
            if !files.is_empty() {
                outpututil::log_success(&format!(
                    "instruction_files valid ({} configured file(s))",
                    files.len()
                ));
            }
        }
        let repo_agents = resolved.repo_root.join("AGENTS.md");
        if repo_agents.exists() {
            outpututil::log_success("AGENTS.md found and injectable");
        }
    } else {
        for warning in instruction_warnings {
            outpututil::log_warn(&warning);
        }
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

/// Check if a runner binary is executable by trying multiple common flags.
///
/// Tries the following in order:
/// 1. `--version`
/// 2. `-V`
/// 3. `--help`
/// 4. `help`
///
/// Returns Ok if any invocation succeeds.
fn check_runner_binary(bin: &str) -> Result<()> {
    let fallbacks: &[&[&str]] = &[&["--version"], &["-V"], &["--help"], &["help"]];

    for args in fallbacks {
        match check_command(bin, args) {
            Ok(()) => return Ok(()),
            Err(_) => continue,
        }
    }

    Err(anyhow::anyhow!(
        "tried: {}",
        fallbacks
            .iter()
            .map(|a| a.join(" "))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Get the config key for a runner's binary path override.
fn get_runner_config_key(runner: Runner) -> &'static str {
    match runner {
        Runner::Codex => "codex_bin",
        Runner::Opencode => "opencode_bin",
        Runner::Gemini => "gemini_bin",
        Runner::Claude => "claude_bin",
        Runner::Cursor => "cursor_bin",
        Runner::Kimi => "kimi_bin",
        Runner::Pi => "pi_bin",
    }
}

/// Check the health of lock directories in .ralph/lock/
///
/// Returns a tuple of (orphaned_count, total_count) where:
/// - orphaned_count: Number of lock directories that appear to be orphaned
/// - total_count: Total number of lock directories found
fn check_lock_health(repo_root: &std::path::Path) -> Result<(usize, usize)> {
    let lock_dir = queue_lock_dir(repo_root);

    if !lock_dir.exists() {
        return Ok((0, 0));
    }

    let mut total_count = 0;
    let mut orphaned_count = 0;

    for entry in fs::read_dir(&lock_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only consider directories
        if !path.is_dir() {
            continue;
        }

        total_count += 1;

        // Check if this lock directory has a valid owner file
        let owner_path = path.join("owner");
        let has_valid_owner = if owner_path.exists() {
            // Check if the owner file has a valid, running PID
            match fs::read_to_string(&owner_path) {
                Ok(content) => {
                    // Parse PID from owner file
                    content
                        .lines()
                        .find(|line| line.starts_with("pid:"))
                        .and_then(|line| line.split(':').nth(1))
                        .and_then(|pid_str| pid_str.trim().parse::<u32>().ok())
                        .and_then(pid_is_running)
                        .unwrap_or(true) // Assume running if we can't determine status
                }
                Err(_) => false,
            }
        } else {
            // Check for task owner files (shared locks)
            let has_task_owner = fs::read_dir(&path)?.any(|e| {
                e.ok()
                    .map(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .map(|name| name.starts_with("owner_task_"))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            });
            has_task_owner
        };

        if !has_valid_owner {
            orphaned_count += 1;
            log::warn!(
                "Orphaned lock directory detected: {} (no valid owner)",
                path.display()
            );
        }
    }

    Ok((orphaned_count, total_count))
}
