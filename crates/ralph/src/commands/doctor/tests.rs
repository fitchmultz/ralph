//! Tests for doctor types and blocking-state aggregation.
//!
//! Responsibilities:
//! - Unit tests for CheckResult factory methods.
//! - Tests for DoctorReport aggregation logic.
//! - Tests for canonical doctor blocking-state derivation.
//!
//! Not handled here:
//! - Integration tests for individual checks (see module tests).
//! - External system validation.
//!
//! Invariants/assumptions:
//! - Blocking-state derivation prefers hard stalls over softer waiting states.

use crate::commands::doctor::{
    derive_check_blocking_state,
    lock::check_lock_health,
    types::{CheckResult, CheckSeverity, DoctorReport},
};
use crate::contracts::{BlockingReason, BlockingState};
use std::path::PathBuf;

fn find_definitely_dead_pid() -> u32 {
    for pid in [0xFFFFFFFE, 999_999, 500_000, 250_000, 100_000] {
        if crate::lock::pid_is_running(pid) == Some(false) {
            return pid;
        }
    }
    panic!("Could not find a definitely-dead PID on this system");
}

fn resolved_with_repo_root(repo_root: PathBuf) -> crate::config::Resolved {
    crate::config::Resolved {
        config: crate::contracts::Config::default(),
        queue_path: repo_root.join(".ralph/queue.jsonc"),
        done_path: repo_root.join(".ralph/done.jsonc"),
        repo_root,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

#[test]
fn check_result_success_factory() {
    let r = CheckResult::success("git", "binary", "git found");
    assert_eq!(r.category, "git");
    assert_eq!(r.check, "binary");
    assert_eq!(r.severity, CheckSeverity::Success);
    assert_eq!(r.message, "git found");
    assert!(!r.fix_available);
    assert!(r.fix_applied.is_none());
    assert!(r.blocking.is_none());
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
    assert!(r.blocking.is_none());
}

#[test]
fn check_result_error_factory() {
    let r = CheckResult::error("git", "repo", "not a git repo", false, Some("run git init"));
    assert_eq!(r.severity, CheckSeverity::Error);
    assert!(!r.fix_available);
    assert!(r.blocking.is_none());
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
fn check_result_with_blocking() {
    let blocking = BlockingState::dependency_blocked(2);
    let result = CheckResult::error("queue", "queue_valid", "queue invalid", false, None)
        .with_blocking(blocking.clone());

    assert_eq!(result.blocking, Some(blocking));
}

#[test]
fn doctor_report_adds_checks() {
    let mut report = DoctorReport::new();
    assert!(report.success);
    assert!(report.blocking.is_none());

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

#[test]
fn doctor_report_serializes_blocking() {
    let mut report = DoctorReport::new();
    report.blocking = Some(BlockingState::idle(false));

    let json = serde_json::to_value(&report).expect("report serializes");
    assert_eq!(json["blocking"]["status"], "waiting");
    assert_eq!(json["blocking"]["reason"]["kind"], "idle");
}

#[test]
fn derive_check_blocking_state_prefers_lock_over_runner_recovery() {
    let runner = BlockingState::runner_recovery(
        "runner",
        "runner_binary_missing",
        None,
        "runner missing",
        "codex not found",
    );
    let lock = BlockingState::lock_blocked(
        Some("/tmp/.ralph/lock".to_string()),
        Some("ralph run loop".to_string()),
        Some(1234),
    );

    let checks = vec![
        CheckResult::error("runner", "runner_binary", &runner.message, false, None)
            .with_blocking(runner),
        CheckResult::error("lock", "queue_lock_held", &lock.message, false, None)
            .with_blocking(lock.clone()),
    ];

    assert_eq!(derive_check_blocking_state(&checks), Some(lock));
}

#[test]
fn derive_check_blocking_state_returns_none_when_no_blockers_exist() {
    let checks = vec![CheckResult::success("git", "binary", "git found")];
    assert!(derive_check_blocking_state(&checks).is_none());
}

#[test]
fn blocking_state_reason_round_trips_through_report_json() {
    let mut report = DoctorReport::new();
    report.blocking = Some(BlockingState::dependency_blocked(3));

    let json = serde_json::to_value(report).expect("report serializes");
    assert_eq!(json["blocking"]["reason"]["kind"], "dependency_blocked");
    assert_eq!(json["blocking"]["reason"]["blocked_tasks"], 3);

    let check_blocking =
        CheckResult::error("queue", "queue_valid", "broken", false, None).with_blocking(
            BlockingState::schedule_blocked(1, Some("2026-12-31T00:00:00Z".to_string()), Some(60)),
        );
    let check_json = serde_json::to_value(check_blocking).expect("check serializes");
    assert_eq!(check_json["blocking"]["reason"]["kind"], "schedule_blocked");
}

#[test]
fn dependency_blocked_reason_shape_matches_contract() {
    let blocking = BlockingState::dependency_blocked(4);
    assert!(matches!(
        blocking.reason,
        BlockingReason::DependencyBlocked { blocked_tasks: 4 }
    ));
}

#[test]
fn check_lock_health_reads_canonical_queue_lock_layout() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let resolved = resolved_with_repo_root(repo_root.clone());
    let lock_dir = repo_root.join(".ralph/lock");
    std::fs::create_dir_all(&lock_dir)?;
    let stale_pid = find_definitely_dead_pid();
    std::fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {stale_pid}\nstarted_at: 2026-02-06T00:56:29Z\ncommand: ralph run loop --max-tasks 0\nlabel: run loop\n"
        ),
    )?;

    let mut report = DoctorReport::new();
    check_lock_health(&mut report, &resolved, false);
    let blocking = report.checks[0].blocking.clone().expect("blocking state");

    assert!(matches!(
        blocking.reason,
        BlockingReason::LockBlocked { .. }
    ));
    assert!(
        blocking.message.contains("stale queue lock"),
        "expected stale queue-lock diagnosis, got: {}",
        blocking.message
    );
    assert!(
        lock_dir.exists(),
        "inspection-only doctor check should not remove the lock"
    );
    Ok(())
}

#[test]
fn check_lock_health_auto_fix_removes_confirmed_stale_queue_lock() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let resolved = resolved_with_repo_root(repo_root.clone());
    let lock_dir = repo_root.join(".ralph/lock");
    std::fs::create_dir_all(&lock_dir)?;
    let stale_pid = find_definitely_dead_pid();
    std::fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {stale_pid}\nstarted_at: 2026-02-06T00:56:29Z\ncommand: ralph run loop --max-tasks 0\nlabel: run loop\n"
        ),
    )?;

    let mut report = DoctorReport::new();
    check_lock_health(&mut report, &resolved, true);

    assert!(
        !lock_dir.exists(),
        "expected stale queue lock to be removed"
    );
    assert_eq!(report.summary.errors, 1);
    assert_eq!(report.summary.fixes_applied, 1);
    assert!(
        report.checks[0].message.contains("stale queue lock"),
        "expected stale lock report, got: {}",
        report.checks[0].message
    );
    Ok(())
}
