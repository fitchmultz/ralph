//! Doctor checks for Git, queue, and runner configuration health.
//!
//! Responsibilities:
//! - Verify Git environment and repository health.
//! - Validate queue and done archive files.
//! - Check runner binary availability and configuration.
//! - Inspect canonical queue-lock health and identify stale or ambiguous ownership.
//! - Check project git hygiene (e.g., sensitive debug logs in `.ralph/logs/` are gitignored).
//! - Apply safe auto-fixes when requested (`--auto-fix` flag).
//! - Derive canonical operator-facing blocking-state diagnostics for doctor surfaces.
//!
//! Not handled here:
//! - Complex repairs requiring user input (prompts go to CLI layer).
//! - Performance benchmarking.
//! - Network connectivity checks.
//!
//! Invariants/assumptions:
//! - All checks are independent; failures in one don't prevent others.
//! - Output uses outpututil for consistent formatting in text mode.
//! - JSON output is machine-readable and stable for scripting.
//! - Auto-fixes are conservative and safe (migrations, queue repair, confirmed stale locks, gitignore updates).
//! - Returns Ok even when blocking is present so callers can inspect the structured report.

pub mod types;

mod git;
mod lock;
mod output;
mod project;
mod queue;
mod runner;

#[cfg(test)]
mod tests;

use crate::config;
use crate::contracts::{BlockingReason, BlockingState};
use crate::queue::operations::RunnableSelectionOptions;
use types::DoctorReport;

pub use output::print_doctor_report_text;
pub use types::CheckResult;
pub use types::CheckSeverity;

/// Run doctor checks and return a structured report.
///
/// When `auto_fix` is true, attempt to fix safe issues:
/// - Run pending config migrations.
/// - Run queue repair for missing fields/invalid timestamps.
/// - Remove confirmed stale queue locks.
pub fn run_doctor(resolved: &config::Resolved, auto_fix: bool) -> anyhow::Result<DoctorReport> {
    log::info!("Running doctor check...");
    let mut report = DoctorReport::new();

    // 1. Git Checks
    log::info!("Checking Git environment...");
    git::check_git(&mut report, resolved);

    // 2. Queue Checks
    log::info!("Checking Ralph queue...");
    queue::check_queue(&mut report, resolved, auto_fix);

    // 3. Done Archive Checks
    log::info!("Checking Ralph done archive...");
    queue::check_done_archive(&mut report, resolved);

    // 4. Lock Health Checks
    log::info!("Checking Ralph lock health...");
    lock::check_lock_health(&mut report, resolved, auto_fix);

    // 5. Runner Checks
    log::info!("Checking Agent configuration...");
    runner::check_runner(&mut report, resolved);

    // 6. Project Checks
    log::info!("Checking project environment...");
    project::check_project(&mut report, resolved, auto_fix);

    report.blocking = derive_doctor_blocking_state(&report, resolved);
    report.success = report
        .checks
        .iter()
        .all(|check| check.severity != CheckSeverity::Error);

    log::info!(
        "Doctor check complete: {} passed, {} warnings, {} errors",
        report.summary.passed,
        report.summary.warnings,
        report.summary.errors
    );

    Ok(report)
}

pub(crate) fn derive_doctor_blocking_state(
    report: &DoctorReport,
    resolved: &config::Resolved,
) -> Option<BlockingState> {
    if let Some(blocking) = derive_check_blocking_state(&report.checks) {
        return Some(blocking);
    }

    if report.checks.iter().any(check_prevents_queue_fallback) {
        return None;
    }

    let active = crate::queue::load_queue(&resolved.queue_path).ok()?;
    let done = if resolved.done_path.exists() {
        Some(crate::queue::load_queue(&resolved.done_path).ok()?)
    } else {
        None
    };

    crate::queue::operations::queue_runnability_report(
        &active,
        done.as_ref(),
        RunnableSelectionOptions::new(false, false),
    )
    .ok()?
    .summary
    .blocking
}

pub(crate) fn derive_check_blocking_state(checks: &[CheckResult]) -> Option<BlockingState> {
    checks
        .iter()
        .filter_map(|check| check.blocking.as_ref())
        .max_by_key(|blocking| blocking_priority(blocking))
        .cloned()
}

fn check_prevents_queue_fallback(check: &CheckResult) -> bool {
    check.severity == CheckSeverity::Error
        && matches!(
            (check.category.as_str(), check.check.as_str()),
            ("queue", "queue_exists")
                | ("queue", "queue_load")
                | ("queue", "queue_valid")
                | ("queue", "done_archive_load")
                | ("queue", "done_archive_valid")
        )
}

fn blocking_priority(blocking: &BlockingState) -> u8 {
    match &blocking.reason {
        BlockingReason::LockBlocked { .. } => 70,
        BlockingReason::CiBlocked { .. } => 60,
        BlockingReason::RunnerRecovery { .. } => 50,
        BlockingReason::OperatorRecovery { .. } => 45,
        BlockingReason::MixedQueue { .. } => 40,
        BlockingReason::DependencyBlocked { .. } => 30,
        BlockingReason::ScheduleBlocked { .. } => 20,
        BlockingReason::Idle { .. } => 10,
    }
}
