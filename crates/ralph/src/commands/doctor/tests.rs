//! Tests for doctor types and report handling.
//!
//! Responsibilities:
//! - Unit tests for CheckResult factory methods
//! - Tests for DoctorReport aggregation logic
//!
//! Not handled here:
//! - Integration tests for individual checks (see module tests)
//! - External system validation

use crate::commands::doctor::types::{CheckResult, CheckSeverity, DoctorReport};

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

    report
        .add(CheckResult::warning("queue", "orphaned", "found", true, None).with_fix_applied(true));
    assert_eq!(report.summary.fixes_applied, 1);

    report
        .add(CheckResult::warning("queue", "another", "found", true, None).with_fix_applied(false));
    assert_eq!(report.summary.fixes_failed, 1);
}
