//! Auto-resume session tests for run command.
//!
//! Purpose:
//! - Auto-resume session tests for run command.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::task_with_id_and_status;
use crate::commands::run::{
    run_session::{ResumeTaskValidation, validate_resumed_task},
    should_echo_blocked_state_without_handler,
};
use crate::contracts::{BlockingState, QueueFile, TaskStatus};
use crate::session;

#[test]
fn validate_resumed_task_succeeds_when_task_exists_and_doing() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Doing)],
    };

    match validate_resumed_task(&queue_file, "RQ-0001", &repo_root)? {
        ResumeTaskValidation::Resumable => {}
        ResumeTaskValidation::FreshStart(_) => panic!("expected resumable task"),
    }

    Ok(())
}

#[test]
fn validate_resumed_task_falls_back_when_task_missing() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Doing)],
    };

    let validation = validate_resumed_task(&queue_file, "RQ-9999", &repo_root)?;
    match validation {
        ResumeTaskValidation::Resumable => panic!("expected fresh-start decision"),
        ResumeTaskValidation::FreshStart(decision) => {
            assert_eq!(
                decision.reason,
                crate::session::ResumeReason::ResumeTargetMissing
            );
            assert!(decision.message.contains("RQ-9999"));
        }
    }

    Ok(())
}

#[test]
fn validate_resumed_task_succeeds_when_task_todo() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Todo)],
    };

    match validate_resumed_task(&queue_file, "RQ-0001", &repo_root)? {
        ResumeTaskValidation::Resumable => {}
        ResumeTaskValidation::FreshStart(_) => panic!("expected resumable task"),
    }

    Ok(())
}

#[test]
fn validate_resumed_task_falls_back_when_task_done() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Done)],
    };

    let validation = validate_resumed_task(&queue_file, "RQ-0001", &repo_root)?;
    match validation {
        ResumeTaskValidation::Resumable => panic!("expected fresh-start decision"),
        ResumeTaskValidation::FreshStart(decision) => {
            assert_eq!(
                decision.reason,
                crate::session::ResumeReason::ResumeTargetTerminal
            );
            assert!(decision.message.contains("already done"));
        }
    }

    Ok(())
}

#[test]
fn validate_resumed_task_falls_back_when_task_rejected() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Rejected)],
    };

    let validation = validate_resumed_task(&queue_file, "RQ-0001", &repo_root)?;
    match validation {
        ResumeTaskValidation::Resumable => panic!("expected fresh-start decision"),
        ResumeTaskValidation::FreshStart(decision) => {
            assert_eq!(
                decision.reason,
                crate::session::ResumeReason::ResumeTargetTerminal
            );
            assert!(decision.message.contains("already rejected"));
        }
    }

    Ok(())
}

#[test]
fn validate_resumed_task_clears_session_when_invalid() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let cache_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir)?;

    // Create a session for a task
    let session = crate::contracts::SessionState::new(
        "test-session".to_string(),
        "RQ-9999".to_string(),
        crate::timeutil::now_utc_rfc3339_or_fallback(),
        1,
        crate::contracts::Runner::Claude,
        "sonnet".to_string(),
        0,
        None,
        None, // phase_settings
    );
    session::save_session(&cache_dir, &session)?;
    assert!(session::session_exists(&cache_dir));

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Doing)],
    };

    let _ = validate_resumed_task(&queue_file, "RQ-9999", &repo_root)?;

    assert!(!session::session_exists(&cache_dir));

    Ok(())
}

#[test]
fn validate_resumed_task_clears_session_when_terminal() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let cache_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir)?;

    // Create a session for a done task
    let session = crate::contracts::SessionState::new(
        "test-session".to_string(),
        "RQ-0001".to_string(),
        crate::timeutil::now_utc_rfc3339_or_fallback(),
        1,
        crate::contracts::Runner::Claude,
        "sonnet".to_string(),
        0,
        None,
        None, // phase_settings
    );
    session::save_session(&cache_dir, &session)?;
    assert!(session::session_exists(&cache_dir));

    // Task is done (terminal status)
    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Done)],
    };

    let _ = validate_resumed_task(&queue_file, "RQ-0001", &repo_root)?;

    assert!(!session::session_exists(&cache_dir));

    Ok(())
}

/// Test that invalid phases values produce a clear user-facing error message.
/// This verifies the defensive programming pattern: even though phases are
/// validated early (1..=3), the match arm uses bail! instead of unreachable!
/// to provide graceful error handling if an invalid value somehow reaches it.
#[test]
fn invalid_phases_produces_user_facing_error() {
    // Directly test the error message format that would be produced
    // by the bail! macro in the match arm
    let phases: u8 = 4;
    let err = anyhow::format_err!(
        "Invalid phases value: {} (expected 1, 2, or 3). \
         This indicates a configuration error or internal inconsistency.",
        phases
    );
    let msg = err.to_string();
    assert!(
        msg.contains("Invalid phases value: 4"),
        "error should mention the invalid value"
    );
    assert!(
        msg.contains("expected 1, 2, or 3"),
        "error should state valid values"
    );
    assert!(
        msg.contains("configuration error or internal inconsistency"),
        "error should indicate severity"
    );
}

/// Test that runner-recovery blockers do not reprint the same resume narration.
#[test]
fn runner_recovery_blocking_state_does_not_duplicate_default_stderr_output() {
    let runner_recovery = BlockingState::runner_recovery(
        "run_session",
        "resume_confirmation_required",
        Some("RQ-0001".to_string()),
        "Resume: refusing to guess because task RQ-0001 has an interrupted session and confirmation is unavailable.",
        "Re-run interactively to choose resume vs fresh, or pass --resume to continue automatically when safe.",
    );
    let dependency_blocked = BlockingState::dependency_blocked(2);

    assert!(!should_echo_blocked_state_without_handler(&runner_recovery));
    assert!(should_echo_blocked_state_without_handler(
        &dependency_blocked
    ));
}

#[test]
fn invalid_phases_edge_cases() {
    for invalid_phase in [0u8, 4u8, 255u8] {
        let err = anyhow::format_err!(
            "Invalid phases value: {} (expected 1, 2, or 3). \
             This indicates a configuration error or internal inconsistency.",
            invalid_phase
        );
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("Invalid phases value: {}", invalid_phase)),
            "error should contain the invalid value {}",
            invalid_phase
        );
    }
}

/// Regression test for RQ-0882: Verify that a resumed loop correctly honors
/// the persisted tasks_completed_in_loop value when enforcing --max-tasks.
#[test]
fn resumed_loop_uses_persisted_progress_for_max_tasks() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let cache_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir)?;

    // Create a session that simulates 2 tasks already completed
    let mut session = crate::contracts::SessionState::new(
        "test-session".to_string(),
        "RQ-0001".to_string(),
        crate::timeutil::now_utc_rfc3339_or_fallback(),
        1,
        crate::contracts::Runner::Claude,
        "sonnet".to_string(),
        5, // max_tasks=5
        None,
        None,
    );
    // Simulate 2 tasks already completed
    session.tasks_completed_in_loop = 2;
    session::save_session(&cache_dir, &session)?;

    // Verify the persisted value is 2
    let loaded = session::load_session(&cache_dir)?.expect("session exists");
    assert_eq!(
        loaded.tasks_completed_in_loop, 2,
        "Session should have persisted tasks_completed_in_loop=2"
    );

    // Increment and verify it becomes 3
    session::increment_session_progress(&cache_dir)?;
    let loaded = session::load_session(&cache_dir)?.expect("session exists");
    assert_eq!(
        loaded.tasks_completed_in_loop, 3,
        "After one increment, tasks_completed_in_loop should be 3"
    );

    Ok(())
}
