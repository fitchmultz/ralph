//! Output formatting for doctor reports.
//!
//! Responsibilities:
//! - Print doctor reports in human-readable text format
//! - Format check results with appropriate severity indicators
//! - Display fix status and suggestions
//!
//! Not handled here:
//! - JSON serialization (handled by serde in types)
//! - Report generation logic
//!
//! Invariants/assumptions:
//! - Uses outpututil for consistent formatting
//! - Respects the report's success state for final messaging

use crate::commands::doctor::types::{CheckSeverity, DoctorReport};
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
