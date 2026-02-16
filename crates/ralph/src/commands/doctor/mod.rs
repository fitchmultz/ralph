//! Doctor checks for Git, queue, and runner configuration health.
//!
//! Responsibilities:
//! - Verify Git environment and repository health
//! - Validate queue and done archive files
//! - Check runner binary availability and configuration
//! - Detect orphaned lock directories that may accumulate over time
//! - Check project git hygiene (e.g., sensitive debug logs in `.ralph/logs/` are gitignored)
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
//! - Auto-fixes are conservative and safe (migrations, queue repair, stale locks, gitignore updates)
//! - Returns Ok only when all critical checks pass

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
use types::DoctorReport;

pub use output::print_doctor_report_text;
pub use types::{CheckResult, CheckSeverity};

/// Run doctor checks and return a structured report.
///
/// When `auto_fix` is true, attempt to fix safe issues:
/// - Run pending config migrations
/// - Run queue repair for missing fields/invalid timestamps
/// - Remove orphaned lock directories
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

    // Update overall success status
    report.success = report
        .checks
        .iter()
        .all(|c| c.severity != types::CheckSeverity::Error);

    log::info!(
        "Doctor check complete: {} passed, {} warnings, {} errors",
        report.summary.passed,
        report.summary.warnings,
        report.summary.errors
    );

    Ok(report)
}
