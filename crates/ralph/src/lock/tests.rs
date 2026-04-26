//! Lock unit tests.
//!
//! Purpose:
//! - Lock unit tests.
//!
//! Responsibilities:
//! - Cover split lock helpers that are easiest to exercise without integration harnesses.
//!
//! Not handled here:
//! - Multi-process integration coverage in `crates/ralph/tests/`.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Current-process PID should be observable on supported platforms.

use super::stale::{LockStalenessAdvisory, classify_lock_owner_at, format_lock_error};
use super::*;

fn test_owner(started_at: &str) -> LockOwner {
    LockOwner {
        pid: 42,
        started_at: started_at.to_string(),
        command: "ralph run loop".to_string(),
        label: "run loop".to_string(),
    }
}

fn find_definitely_dead_pid() -> u32 {
    for pid in [0xFFFF_FFFE, 999_999, 500_000, 250_000, 100_000] {
        if pid_is_running(pid) == Some(false) {
            return pid;
        }
    }
    panic!("Could not find a definitely-dead PID on this system");
}

#[test]
fn pid_is_running_current_process() {
    let current_pid = std::process::id();
    assert_eq!(pid_is_running(current_pid), Some(true));
}

#[test]
fn pid_is_running_nonexistent_pid_never_reports_running() {
    assert_ne!(pid_is_running(0xFFFF_FFFE), Some(true));
}

#[test]
fn pid_is_running_system_idle_is_not_definitively_dead() {
    assert_ne!(pid_is_running(0), Some(false));
}

#[test]
fn is_task_owner_file_matches_expected_patterns() {
    assert!(is_task_owner_file("owner_task_1234"));
    assert!(is_task_owner_file("owner_task_1234_0"));
    assert!(is_task_owner_file("owner_task_1234_42"));
    assert!(!is_task_owner_file("owner"));
    assert!(!is_task_owner_file("owner_other"));
    assert!(!is_task_owner_file("owner_task"));
    assert!(!is_task_owner_file(""));
    assert!(!is_task_owner_file("task_owner_1234"));
}

#[test]
fn pid_liveness_helpers_are_consistent() {
    assert!(PidLiveness::NotRunning.is_definitely_not_running());
    assert!(!PidLiveness::Running.is_definitely_not_running());
    assert!(!PidLiveness::Indeterminate.is_definitely_not_running());

    assert!(PidLiveness::Running.is_running_or_indeterminate());
    assert!(PidLiveness::Indeterminate.is_running_or_indeterminate());
    assert!(!PidLiveness::NotRunning.is_running_or_indeterminate());
}

#[test]
fn pid_liveness_wraps_pid_is_running() {
    assert_eq!(pid_liveness(std::process::id()), PidLiveness::Running);
    assert_ne!(pid_liveness(0xFFFF_FFFE), PidLiveness::Running);
}

#[test]
fn lock_staleness_only_auto_stales_definitely_dead_pid() {
    let now = crate::timeutil::parse_rfc3339("2026-04-17T00:00:00Z").unwrap();
    let owner = test_owner("not-a-timestamp");

    let staleness = classify_lock_owner_at(&owner, now, PidLiveness::NotRunning);

    assert!(staleness.is_stale());
    assert_eq!(staleness.advisory, LockStalenessAdvisory::None);
}

#[test]
fn lock_staleness_flags_aged_live_pid_for_review_without_auto_stale() {
    let now = crate::timeutil::parse_rfc3339("2026-04-17T00:00:00Z").unwrap();
    let owner = test_owner("2026-04-09T00:00:00Z");

    let staleness = classify_lock_owner_at(&owner, now, PidLiveness::Running);

    assert!(!staleness.is_stale());
    assert_eq!(staleness.advisory, LockStalenessAdvisory::AgedLivePid);
}

#[test]
fn lock_staleness_flags_unclear_owner_time_for_review_without_auto_stale() {
    let now = crate::timeutil::parse_rfc3339("2026-04-17T00:00:00Z").unwrap();

    let invalid = classify_lock_owner_at(&test_owner("unknown"), now, PidLiveness::Indeterminate);
    assert!(!invalid.is_stale());
    assert_eq!(invalid.advisory, LockStalenessAdvisory::InvalidStartedAt);

    let future = classify_lock_owner_at(
        &test_owner("2026-04-17T00:06:00Z"),
        now,
        PidLiveness::Running,
    );
    assert!(!future.is_stale());
    assert_eq!(future.advisory, LockStalenessAdvisory::FutureStartedAt);
}

#[test]
fn acquire_dir_lock_auto_clears_stale_lock_without_force() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let lock_dir = temp.path().join("lock");
    std::fs::create_dir_all(&lock_dir)?;
    std::fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {}\nstarted_at: 2026-02-06T00:56:29Z\ncommand: ralph run loop --max-tasks 0\nlabel: run loop\n",
            find_definitely_dead_pid()
        ),
    )?;

    let lock = acquire_dir_lock(&lock_dir, "next run", false)?;

    let owner = std::fs::read_to_string(lock_dir.join("owner"))?;
    assert!(
        owner.contains("label: next run"),
        "expected acquisition to replace stale owner metadata, got: {owner}"
    );
    drop(lock);
    assert!(
        !lock_dir.exists(),
        "expected new lock guard to clean up normally after drop"
    );

    Ok(())
}

#[test]
fn lock_error_suggestions_do_not_emit_manual_rm_commands() {
    let owner = test_owner("2026-04-09T00:00:00Z");
    let message = format_lock_error(
        std::path::Path::new("/tmp/ralph-lock"),
        Some(&owner),
        true,
        false,
        Some(LockStaleness {
            liveness: PidLiveness::NotRunning,
            advisory: LockStalenessAdvisory::None,
        }),
    );

    assert!(message.contains("ralph queue unlock --yes"));
    assert!(!message.contains("rm -rf"), "message was: {message}");
}

#[test]
fn lock_error_explains_pid_reuse_review_policy() {
    let now = crate::timeutil::parse_rfc3339("2026-04-17T00:00:00Z").unwrap();
    let owner = test_owner("2026-04-09T00:00:00Z");
    let staleness = classify_lock_owner_at(&owner, now, PidLiveness::Running);

    let message = format_lock_error(
        std::path::Path::new("/tmp/ralph-lock"),
        Some(&owner),
        staleness.is_stale(),
        false,
        Some(staleness),
    );

    assert!(message.contains("PID REUSE REVIEW"));
    assert!(message.contains("Ralph does not auto-clear it"));
    assert!(message.contains("verify the PID, command, and timestamp"));
}
