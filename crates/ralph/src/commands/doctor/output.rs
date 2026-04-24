//! Output formatting for doctor reports.
//!
//! Purpose:
//! - Output formatting for doctor reports.
//!
//! Responsibilities:
//! - Print doctor reports in human-readable text format.
//! - Format check results with appropriate severity indicators.
//! - Display fix status, suggestions, and canonical blocking-state diagnosis.
//!
//! Not handled here:
//! - JSON serialization (handled by serde in types).
//! - Report generation logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Uses outpututil for consistent formatting.
//! - Respects the report's success state for final messaging.
//! - Blocking-state narration reuses the canonical contracts vocabulary.

use crate::commands::doctor::types::{CheckSeverity, DoctorReport};
use crate::contracts::{BlockingReason, BlockingState, BlockingStatus};
use crate::outpututil;

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

    if let Some(blocking) = &report.blocking {
        render_blocking_state(blocking);
    }

    if report.summary.fixes_applied > 0 {
        log::info!("Applied {} auto-fix(es)", report.summary.fixes_applied);
    }
    if report.summary.fixes_failed > 0 {
        log::warn!("Failed to apply {} fix(es)", report.summary.fixes_failed);
    }

    match (&report.blocking, report.success) {
        (None, true) => {
            log::info!("Doctor check passed. System is ready.");
        }
        (Some(blocking), true) => {
            let summary = match blocking.status {
                BlockingStatus::Waiting => {
                    "Doctor check passed. Ralph is healthy, but it is currently waiting."
                }
                BlockingStatus::Blocked => {
                    "Doctor check passed. Ralph is healthy, but current work is blocked."
                }
                BlockingStatus::Stalled => {
                    "Doctor check passed, but Ralph is stalled and needs operator attention."
                }
            };
            match blocking.status {
                BlockingStatus::Waiting => log::info!("{}", summary),
                BlockingStatus::Blocked => log::warn!("{}", summary),
                BlockingStatus::Stalled => outpututil::log_error(summary),
            }
        }
        (_, false) => {
            outpututil::log_error(&format!("Doctor found {} issue(s):", report.summary.errors));
        }
    }
}

fn render_blocking_state(blocking: &BlockingState) {
    let header = match blocking.status {
        BlockingStatus::Waiting => "Blocking state: waiting",
        BlockingStatus::Blocked => "Blocking state: blocked",
        BlockingStatus::Stalled => "Blocking state: stalled",
    };

    match blocking.status {
        BlockingStatus::Waiting => outpututil::log_warn(header),
        BlockingStatus::Blocked | BlockingStatus::Stalled => outpututil::log_error(header),
    }

    log::info!("  Reason: {}", blocking_reason_name(&blocking.reason));
    log::info!("  {}", blocking.message);
    if !blocking.detail.trim().is_empty() {
        log::info!("  {}", blocking.detail);
    }
    if let Some(task_id) = &blocking.task_id {
        log::info!("  Task: {}", task_id);
    }
}

fn blocking_reason_name(reason: &BlockingReason) -> &'static str {
    match reason {
        BlockingReason::Idle { .. } => "idle",
        BlockingReason::DependencyBlocked { .. } => "dependency_blocked",
        BlockingReason::ScheduleBlocked { .. } => "schedule_blocked",
        BlockingReason::LockBlocked { .. } => "lock_blocked",
        BlockingReason::CiBlocked { .. } => "ci_blocked",
        BlockingReason::RunnerRecovery { .. } => "runner_recovery",
        BlockingReason::OperatorRecovery { .. } => "operator_recovery",
        BlockingReason::MixedQueue { .. } => "mixed_queue",
    }
}
