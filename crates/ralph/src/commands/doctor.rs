//! Doctor checks for Git, queue, and runner configuration health.
//!
//! Responsibilities:
//! - Verify Git environment and repository health
//! - Validate queue and done archive files
//! - Check runner binary availability and configuration
//! - Detect orphaned lock directories that may accumulate over time
//! - Apply safe auto-fixes when requested (--auto-fix flag)
//!
//! Not handled here:
//! - Complex repairs requiring user input (prompts go to CLI layer)
//! - Performance benchmarking
//! - Network connectivity checks
//!
//! Invariants/assumptions:
//! - All checks are independent; failures in one don't prevent others
//! - Output uses outpututil for consistent formatting in text mode
//! - JSON output is machine-readable and stable for scripting
//! - Auto-fixes are conservative and safe (migrations, queue repair, stale locks)
//! - Returns Ok only when all critical checks pass

use crate::config;
use crate::contracts::Runner;
use crate::git;
use crate::lock::{is_task_owner_file, pid_is_running, queue_lock_dir};
use crate::outpututil;
use crate::prompts;
use crate::queue;
use crate::runner;
use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Severity level for a doctor check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum CheckSeverity {
    /// Check passed successfully.
    Success,
    /// Non-critical issue, operation can continue.
    Warning,
    /// Critical issue, operation should not proceed.
    Error,
}

/// A single check result.
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    /// Category of the check (git, queue, runner, project, lock).
    pub category: String,
    /// Specific check name (e.g., "git_binary", "queue_valid").
    pub check: String,
    /// Severity level of the result.
    pub severity: CheckSeverity,
    /// Human-readable message describing the result.
    pub message: String,
    /// Whether a fix is available for this issue.
    pub fix_available: bool,
    /// Whether a fix was applied (None if not attempted, Some(true/false) if attempted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_applied: Option<bool>,
    /// Suggested fix or action for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
}

impl CheckResult {
    /// Create a successful check result.
    pub fn success(category: &str, check: &str, message: &str) -> Self {
        Self {
            category: category.to_string(),
            check: check.to_string(),
            severity: CheckSeverity::Success,
            message: message.to_string(),
            fix_available: false,
            fix_applied: None,
            suggested_fix: None,
        }
    }

    /// Create a warning check result.
    pub fn warning(
        category: &str,
        check: &str,
        message: &str,
        fix_available: bool,
        suggested_fix: Option<&str>,
    ) -> Self {
        Self {
            category: category.to_string(),
            check: check.to_string(),
            severity: CheckSeverity::Warning,
            message: message.to_string(),
            fix_available,
            fix_applied: None,
            suggested_fix: suggested_fix.map(|s| s.to_string()),
        }
    }

    /// Create an error check result.
    pub fn error(
        category: &str,
        check: &str,
        message: &str,
        fix_available: bool,
        suggested_fix: Option<&str>,
    ) -> Self {
        Self {
            category: category.to_string(),
            check: check.to_string(),
            severity: CheckSeverity::Error,
            message: message.to_string(),
            fix_available,
            fix_applied: None,
            suggested_fix: suggested_fix.map(|s| s.to_string()),
        }
    }

    /// Mark that a fix was applied to this check.
    pub fn with_fix_applied(mut self, applied: bool) -> Self {
        self.fix_applied = Some(applied);
        self
    }
}

/// Summary of all checks.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    /// Total number of checks performed.
    pub total: usize,
    /// Number of successful checks.
    pub passed: usize,
    /// Number of warnings.
    pub warnings: usize,
    /// Number of errors.
    pub errors: usize,
    /// Number of fixes applied.
    pub fixes_applied: usize,
    /// Number of fixes that failed.
    pub fixes_failed: usize,
}

/// Full doctor report (for JSON output).
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    /// Overall success status (true if no errors).
    pub success: bool,
    /// Individual check results.
    pub checks: Vec<CheckResult>,
    /// Summary statistics.
    pub summary: Summary,
}

impl DoctorReport {
    /// Create a new empty report.
    pub fn new() -> Self {
        Self {
            success: true,
            checks: Vec::new(),
            summary: Summary {
                total: 0,
                passed: 0,
                warnings: 0,
                errors: 0,
                fixes_applied: 0,
                fixes_failed: 0,
            },
        }
    }

    /// Add a check result to the report.
    pub fn add(&mut self, result: CheckResult) {
        self.summary.total += 1;
        match result.severity {
            CheckSeverity::Success => self.summary.passed += 1,
            CheckSeverity::Warning => self.summary.warnings += 1,
            CheckSeverity::Error => {
                self.summary.errors += 1;
                self.success = false;
            }
        }
        if result.fix_applied == Some(true) {
            self.summary.fixes_applied += 1;
        } else if result.fix_applied == Some(false) {
            self.summary.fixes_failed += 1;
        }
        self.checks.push(result);
    }
}

impl Default for DoctorReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Run doctor checks and return a structured report.
///
/// When `auto_fix` is true, attempt to fix safe issues:
/// - Run pending config migrations
/// - Run queue repair for missing fields/invalid timestamps
/// - Remove orphaned lock directories
pub fn run_doctor(resolved: &config::Resolved, auto_fix: bool) -> Result<DoctorReport> {
    log::info!("Running doctor check...");
    let mut report = DoctorReport::new();

    // 1. Git Checks
    log::info!("Checking Git environment...");
    check_git(&mut report, resolved);

    // 2. Queue Checks
    log::info!("Checking Ralph queue...");
    check_queue(&mut report, resolved, auto_fix);

    // 3. Done Archive Checks
    log::info!("Checking Ralph done archive...");
    check_done_archive(&mut report, resolved);

    // 4. Lock Health Checks
    log::info!("Checking Ralph lock health...");
    check_lock_health(&mut report, resolved, auto_fix);

    // 5. Runner Checks
    log::info!("Checking Agent configuration...");
    check_runner(&mut report, resolved);

    // 6. Project Checks
    log::info!("Checking project environment...");
    check_project(&mut report, resolved);

    // Update overall success status
    report.success = report
        .checks
        .iter()
        .all(|c| c.severity != CheckSeverity::Error);

    log::info!(
        "Doctor check complete: {} passed, {} warnings, {} errors",
        report.summary.passed,
        report.summary.warnings,
        report.summary.errors
    );

    Ok(report)
}

/// Print doctor report in human-readable text format.
pub fn print_doctor_report_text(report: &DoctorReport) {
    for check in &report.checks {
        match check.severity {
            CheckSeverity::Success => {
                outpututil::log_success(&check.message);
            }
            CheckSeverity::Warning => {
                outpututil::log_warn(&check.message);
            }
            CheckSeverity::Error => {
                outpututil::log_error(&check.message);
            }
        }

        // Print fix information if relevant
        if let Some(fix_applied) = check.fix_applied {
            if fix_applied {
                outpututil::log_success(&format!("  [FIXED] {}", check.message));
            } else {
                outpututil::log_error("  [FIX FAILED] Unable to apply fix");
            }
        } else if check.fix_available
            && !report.success
            && let Some(ref suggestion) = check.suggested_fix
        {
            log::info!("  Suggested fix: {}", suggestion);
        }
    }

    // Print summary
    if report.summary.fixes_applied > 0 {
        log::info!("Applied {} auto-fix(es)", report.summary.fixes_applied);
    }
    if report.summary.fixes_failed > 0 {
        log::warn!("Failed to apply {} fix(es)", report.summary.fixes_failed);
    }

    if report.success {
        log::info!("Doctor check passed. System is ready.");
    } else {
        outpututil::log_error(&format!("Doctor found {} issue(s):", report.summary.errors));
        for check in &report.checks {
            if check.severity == CheckSeverity::Error {
                log::error!("  - {}", check.message);
            }
        }
    }
}

fn check_git(report: &mut DoctorReport, resolved: &config::Resolved) {
    // Check git binary
    if let Err(e) = check_command("git", &["--version"]) {
        report.add(CheckResult::error(
            "git",
            "git_binary",
            &format!("git binary not found or not executable: {}", e),
            false,
            Some("Install git and ensure it's in your PATH"),
        ));
    } else {
        report.add(CheckResult::success(
            "git",
            "git_binary",
            "git binary found",
        ));
    }

    // Check valid git repo
    match git::status_porcelain(&resolved.repo_root) {
        Ok(_) => {
            report.add(CheckResult::success(
                "git",
                "git_repo",
                &format!("valid git repo at {}", resolved.repo_root.display()),
            ));
        }
        Err(e) => {
            report.add(CheckResult::error(
                "git",
                "git_repo",
                &format!("invalid git repo: {}", e),
                false,
                Some("Run 'git init' to initialize a git repository"),
            ));
        }
    }

    // Check upstream configuration
    match git::upstream_ref(&resolved.repo_root) {
        Ok(u) => {
            report.add(CheckResult::success(
                "git",
                "upstream_config",
                &format!("upstream configured: {}", u),
            ));
        }
        Err(e) => {
            report.add(CheckResult::warning(
                "git",
                "upstream_config",
                &format!("no upstream configured: {}", e),
                false,
                Some("Set up a remote upstream with 'git remote add origin <url>'"),
            ));
        }
    }

    // Git LFS Checks
    match git::has_lfs(&resolved.repo_root) {
        Ok(true) => {
            report.add(CheckResult::success("git", "git_lfs", "Git LFS detected"));
            match git::list_lfs_files(&resolved.repo_root) {
                Ok(files) => {
                    if files.is_empty() {
                        log::info!("LFS initialized but no files tracked");
                    } else {
                        report.add(CheckResult::success(
                            "git",
                            "lfs_files",
                            &format!("LFS tracking {} file(s)", files.len()),
                        ));
                    }
                }
                Err(e) => {
                    report.add(CheckResult::warning(
                        "git",
                        "lfs_files",
                        &format!("Failed to list LFS files: {}", e),
                        false,
                        None,
                    ));
                }
            }
        }
        Ok(false) => {
            log::info!("Git LFS not detected");
        }
        Err(e) => {
            report.add(CheckResult::warning(
                "git",
                "git_lfs",
                &format!("LFS check failed: {}", e),
                false,
                None,
            ));
        }
    }
}

fn check_queue(report: &mut DoctorReport, resolved: &config::Resolved, auto_fix: bool) {
    if !resolved.queue_path.exists() {
        report.add(CheckResult::error(
            "queue",
            "queue_exists",
            &format!("queue file missing at {}", resolved.queue_path.display()),
            false,
            Some("Run 'ralph init' to create a new queue"),
        ));
        return;
    }

    match queue::load_queue(&resolved.queue_path) {
        Ok(q) => {
            match queue::validate_queue(&q, &resolved.id_prefix, resolved.id_width) {
                Ok(_) => {
                    report.add(CheckResult::success(
                        "queue",
                        "queue_valid",
                        &format!("queue valid ({} tasks)", q.tasks.len()),
                    ));
                }
                Err(e) => {
                    // Queue validation failed - offer repair as auto-fix
                    let fix_available = true;

                    if auto_fix && fix_available {
                        match apply_queue_repair(resolved) {
                            Ok(repair_report) => {
                                log::info!(
                                    "Queue repair applied: {} tasks fixed, {} timestamps fixed, {} IDs remapped",
                                    repair_report.fixed_tasks,
                                    repair_report.fixed_timestamps,
                                    repair_report.remapped_ids.len()
                                );

                                // Re-validate the queue after repair
                                match queue::load_queue(&resolved.queue_path) {
                                    Ok(repaired_q) => {
                                        match queue::validate_queue(
                                            &repaired_q,
                                            &resolved.id_prefix,
                                            resolved.id_width,
                                        ) {
                                            Ok(_) => {
                                                // Repair succeeded and queue is now valid
                                                report.add(CheckResult::success(
                                                    "queue",
                                                    "queue_valid",
                                                    &format!(
                                                        "queue valid after repair ({} tasks)",
                                                        repaired_q.tasks.len()
                                                    ),
                                                ));
                                            }
                                            Err(reval_err) => {
                                                // Repair was applied but validation still fails
                                                report.add(
                                                    CheckResult::error(
                                                        "queue",
                                                        "queue_valid",
                                                        &format!(
                                                            "queue validation failed: {}",
                                                            reval_err
                                                        ),
                                                        false,
                                                        Some("Manual repair required"),
                                                    )
                                                    .with_fix_applied(false),
                                                );
                                            }
                                        }
                                    }
                                    Err(load_err) => {
                                        report.add(
                                            CheckResult::error(
                                                "queue",
                                                "queue_load",
                                                &format!("failed to load queue after repair: {}", load_err),
                                                false,
                                                Some("Check queue file format or restore from backup"),
                                            )
                                            .with_fix_applied(false),
                                        );
                                    }
                                }
                            }
                            Err(repair_err) => {
                                log::error!("Failed to repair queue: {}", repair_err);
                                report.add(
                                    CheckResult::error(
                                        "queue",
                                        "queue_valid",
                                        &format!("queue validation failed: {}", e),
                                        fix_available,
                                        Some("Run 'ralph queue repair' to repair"),
                                    )
                                    .with_fix_applied(false),
                                );
                            }
                        }
                    } else {
                        // No auto-fix, report the error
                        report.add(CheckResult::error(
                            "queue",
                            "queue_valid",
                            &format!("queue validation failed: {}", e),
                            fix_available,
                            Some("Run 'ralph queue repair' or use --auto-fix to repair automatically"),
                        ));
                    }
                }
            }
        }
        Err(e) => {
            report.add(CheckResult::error(
                "queue",
                "queue_load",
                &format!("failed to load queue: {}", e),
                false,
                Some("Check queue file format or restore from backup"),
            ));
        }
    }
}

fn check_done_archive(report: &mut DoctorReport, resolved: &config::Resolved) {
    if !resolved.done_path.exists() {
        log::info!("done archive missing (optional)");
        return;
    }

    match queue::load_queue(&resolved.done_path) {
        Ok(d) => match queue::validate_queue(&d, &resolved.id_prefix, resolved.id_width) {
            Ok(_) => {
                report.add(CheckResult::success(
                    "queue",
                    "done_archive_valid",
                    &format!("done archive valid ({} tasks)", d.tasks.len()),
                ));
            }
            Err(e) => {
                report.add(CheckResult::error(
                    "queue",
                    "done_archive_valid",
                    &format!("done archive validation failed: {}", e),
                    false,
                    Some("Run 'ralph queue repair' to repair the done archive"),
                ));
            }
        },
        Err(e) => {
            report.add(CheckResult::error(
                "queue",
                "done_archive_load",
                &format!("failed to load done archive: {}", e),
                false,
                Some("Check done file format or restore from backup"),
            ));
        }
    }
}

fn check_lock_health(report: &mut DoctorReport, resolved: &config::Resolved, auto_fix: bool) {
    match check_lock_directory_health(&resolved.repo_root) {
        Ok((orphaned_count, total_count)) => {
            if orphaned_count > 0 {
                let fix_available = true;
                let mut result = CheckResult::warning(
                    "lock",
                    "orphaned_locks",
                    &format!(
                        "found {} orphaned lock director{} (out of {} total)",
                        orphaned_count,
                        if orphaned_count == 1 { "y" } else { "ies" },
                        total_count
                    ),
                    fix_available,
                    Some("Use --auto-fix to remove orphaned lock directories"),
                );

                if auto_fix && fix_available {
                    match remove_orphaned_locks(&resolved.repo_root) {
                        Ok(removed_count) => {
                            log::info!("Removed {} orphaned lock directories", removed_count);
                            result = result.with_fix_applied(true);
                        }
                        Err(remove_err) => {
                            log::error!("Failed to remove orphaned locks: {}", remove_err);
                            result = result.with_fix_applied(false);
                        }
                    }
                }

                report.add(result);
            } else if total_count > 0 {
                report.add(CheckResult::success(
                    "lock",
                    "lock_health",
                    &format!(
                        "all {} lock director{} healthy",
                        total_count,
                        if total_count == 1 { "y" } else { "ies" }
                    ),
                ));
            } else {
                log::info!("no lock directories found");
            }
        }
        Err(e) => {
            report.add(CheckResult::warning(
                "lock",
                "lock_health",
                &format!("lock health check failed: {}", e),
                false,
                None,
            ));
        }
    }
}

fn check_runner(report: &mut DoctorReport, resolved: &config::Resolved) {
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
            let result = CheckResult::error(
                "runner",
                "runner_binary",
                &message,
                false,
                Some(&format!(
                    "Install the runner binary, or configure a custom path in .ralph/config.json: {{ \"agent\": {{ \"{}\": \"/path/to/{}\" }} }}",
                    config_key, bin_name
                )),
            );
            report.add(result);
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
        } else {
            let result = CheckResult::warning(
                "runner",
                "runner_binary",
                &message,
                false,
                Some("Install the runner binary, or configure a custom path in .ralph/config.json"),
            );
            report.add(result);
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
        report.add(CheckResult::success(
            "runner",
            "runner_binary",
            &format!("runner binary '{}' ({:?}) found", bin_name, runner),
        ));
    }

    // Model Compatibility Check
    let model = runner::resolve_model_for_runner(
        runner,
        None,
        None,
        resolved.config.agent.model.clone(),
        false,
    );
    if let Err(e) = runner::validate_model_for_runner(runner, &model) {
        report.add(CheckResult::error(
            "runner",
            "model_compatibility",
            &format!("config model/runner mismatch: {}", e),
            false,
            Some("Check the model is compatible with the selected runner in config"),
        ));
    } else {
        report.add(CheckResult::success(
            "runner",
            "model_compatibility",
            &format!(
                "model '{}' compatible with runner '{:?}'",
                model.as_str(),
                runner
            ),
        ));
    }

    // Instruction file injection checks
    let instruction_warnings =
        prompts::instruction_file_warnings(&resolved.repo_root, &resolved.config);

    // Check if repo AGENTS.md is explicitly configured
    let repo_agents_configured = resolved
        .config
        .agent
        .instruction_files
        .as_ref()
        .map(|files| {
            files.iter().any(|p| {
                let resolved = resolved.repo_root.join(p);
                resolved.ends_with("AGENTS.md")
            })
        })
        .unwrap_or(false);
    let repo_agents_path = resolved.repo_root.join("AGENTS.md");
    let repo_agents_exists = repo_agents_path.exists();

    if instruction_warnings.is_empty() {
        if let Some(files) = resolved.config.agent.instruction_files.as_ref()
            && !files.is_empty()
        {
            report.add(CheckResult::success(
                "runner",
                "instruction_files",
                &format!(
                    "instruction_files valid ({} configured file(s))",
                    files.len()
                ),
            ));
        }
        // Report status of repo AGENTS.md based on configuration
        if repo_agents_configured && repo_agents_exists {
            report.add(CheckResult::success(
                "runner",
                "agents_md",
                "AGENTS.md configured and readable",
            ));
        } else if repo_agents_exists && !repo_agents_configured {
            report.add(CheckResult::warning(
                "runner",
                "agents_md",
                "AGENTS.md exists at repo root but is not configured for injection. \
                 To enable, add 'AGENTS.md' to agent.instruction_files in your config.",
                false,
                Some("Add 'AGENTS.md' to agent.instruction_files in .ralph/config.json"),
            ));
        }
    } else {
        for warning in instruction_warnings {
            report.add(CheckResult::warning(
                "runner",
                "instruction_files",
                &warning,
                false,
                Some("Check instruction file paths in config"),
            ));
        }
    }
}

fn check_project(report: &mut DoctorReport, resolved: &config::Resolved) {
    let makefile_path = resolved.repo_root.join("Makefile");
    if makefile_path.exists() {
        report.add(CheckResult::success(
            "project",
            "makefile",
            "Makefile found",
        ));
        match fs::read_to_string(&makefile_path) {
            Ok(content) => {
                if content.contains("ci:") {
                    report.add(CheckResult::success(
                        "project",
                        "ci_target",
                        "Makefile has 'ci' target",
                    ));
                } else {
                    report.add(CheckResult::warning(
                        "project",
                        "ci_target",
                        "Makefile exists but missing 'ci' target",
                        false,
                        Some("Add a 'ci' target to your Makefile for automated checks"),
                    ));
                }
            }
            Err(e) => {
                report.add(CheckResult::error(
                    "project",
                    "makefile_read",
                    &format!("failed to read Makefile: {}", e),
                    false,
                    Some("Check file permissions"),
                ));
            }
        }
    } else {
        report.add(CheckResult::error(
            "project",
            "makefile",
            "Makefile missing in repo root",
            false,
            Some("Create a Makefile with a 'ci' target"),
        ));
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

    if let Some(path) = resolved.global_config_path.as_ref()
        && path.exists()
    {
        consider_layer(path);
    }
    if let Some(path) = resolved.project_config_path.as_ref()
        && path.exists()
    {
        consider_layer(path);
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

/// Apply queue repair for auto-fix.
fn apply_queue_repair(resolved: &config::Resolved) -> Result<queue::repair::RepairReport> {
    queue::repair::repair_queue(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
        false, // not dry run
    )
}

/// Check the health of lock directories in .ralph/lock/
///
/// Returns a tuple of (orphaned_count, total_count) where:
/// - orphaned_count: Number of lock directories that appear to be orphaned
/// - total_count: Total number of lock directories found
fn check_lock_directory_health(repo_root: &Path) -> Result<(usize, usize)> {
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
            // Use the shared helper from lock module to detect task sidecar files
            fs::read_dir(&path)?.any(|e| {
                e.ok()
                    .map(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .map(is_task_owner_file)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
        };

        if !has_valid_owner {
            orphaned_count += 1;
            log::debug!(
                "Orphaned lock directory detected: {} (no valid owner)",
                path.display()
            );
        }
    }

    Ok((orphaned_count, total_count))
}

/// Remove orphaned lock directories.
///
/// Returns the number of directories removed.
fn remove_orphaned_locks(repo_root: &Path) -> Result<usize> {
    let lock_dir = queue_lock_dir(repo_root);

    if !lock_dir.exists() {
        return Ok(0);
    }

    let mut removed_count = 0;

    for entry in fs::read_dir(&lock_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only consider directories
        if !path.is_dir() {
            continue;
        }

        // Check if this lock directory has a valid owner file
        let owner_path = path.join("owner");
        let has_valid_owner = if owner_path.exists() {
            match fs::read_to_string(&owner_path) {
                Ok(content) => content
                    .lines()
                    .find(|line| line.starts_with("pid:"))
                    .and_then(|line| line.split(':').nth(1))
                    .and_then(|pid_str| pid_str.trim().parse::<u32>().ok())
                    .and_then(pid_is_running)
                    .unwrap_or(true),
                Err(_) => false,
            }
        } else {
            fs::read_dir(&path)?.any(|e| {
                e.ok()
                    .map(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .map(is_task_owner_file)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
        };

        if !has_valid_owner {
            log::info!("Removing orphaned lock directory: {}", path.display());
            fs::remove_dir_all(&path)?;
            removed_count += 1;
        }
    }

    // Try to clean up the lock directory itself if it's now empty
    if lock_dir.exists() {
        let is_empty = fs::read_dir(&lock_dir)?.next().is_none();
        if is_empty {
            fs::remove_dir(&lock_dir)?;
        }
    }

    Ok(removed_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_result_success_factory() {
        let r = CheckResult::success("git", "binary", "git found");
        assert_eq!(r.category, "git");
        assert_eq!(r.check, "binary");
        assert_eq!(r.severity, CheckSeverity::Success);
        assert_eq!(r.message, "git found");
        assert!(!r.fix_available);
        assert!(r.fix_applied.is_none());
    }

    #[test]
    fn check_result_warning_factory() {
        let r = CheckResult::warning(
            "queue",
            "orphaned",
            "found orphaned locks",
            true,
            Some("run repair"),
        );
        assert_eq!(r.severity, CheckSeverity::Warning);
        assert!(r.fix_available);
        assert_eq!(r.suggested_fix, Some("run repair".to_string()));
    }

    #[test]
    fn check_result_error_factory() {
        let r = CheckResult::error("git", "repo", "not a git repo", false, Some("run git init"));
        assert_eq!(r.severity, CheckSeverity::Error);
        assert!(!r.fix_available);
    }

    #[test]
    fn check_result_with_fix_applied() {
        let r = CheckResult::warning(
            "queue",
            "orphaned",
            "found orphaned locks",
            true,
            Some("run repair"),
        )
        .with_fix_applied(true);
        assert_eq!(r.fix_applied, Some(true));
    }

    #[test]
    fn doctor_report_adds_checks() {
        let mut report = DoctorReport::new();
        assert!(report.success);

        report.add(CheckResult::success("git", "binary", "git found"));
        assert_eq!(report.summary.total, 1);
        assert_eq!(report.summary.passed, 1);
        assert!(report.success);

        report.add(CheckResult::warning(
            "queue",
            "orphaned",
            "found orphaned",
            true,
            None,
        ));
        assert_eq!(report.summary.warnings, 1);
        assert!(report.success);

        report.add(CheckResult::error(
            "git",
            "repo",
            "not a git repo",
            false,
            None,
        ));
        assert_eq!(report.summary.errors, 1);
        assert!(!report.success);
    }

    #[test]
    fn doctor_report_tracks_fixes() {
        let mut report = DoctorReport::new();

        report.add(
            CheckResult::warning("queue", "orphaned", "found", true, None).with_fix_applied(true),
        );
        assert_eq!(report.summary.fixes_applied, 1);

        report.add(
            CheckResult::warning("queue", "another", "found", true, None).with_fix_applied(false),
        );
        assert_eq!(report.summary.fixes_failed, 1);
    }
}
