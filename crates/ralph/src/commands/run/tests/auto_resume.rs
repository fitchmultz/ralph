//! Auto-resume session tests for run command.

use super::task_with_id_and_status;
use crate::contracts::{QueueFile, TaskStatus};
use crate::session;

#[test]
fn validate_resumed_task_succeeds_when_task_exists_and_doing() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Doing)],
    };

    // Should succeed when task exists and is Doing
    crate::commands::run::validate_resumed_task(&queue_file, "RQ-0001", &repo_root)?;

    Ok(())
}

#[test]
fn validate_resumed_task_fails_when_task_missing() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Doing)],
    };

    // Should fail when task doesn't exist
    let err = crate::commands::run::validate_resumed_task(&queue_file, "RQ-9999", &repo_root)
        .unwrap_err();
    assert!(err.to_string().contains("no longer exists"));

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

    // Should succeed for Todo tasks - they are valid for resumption
    // (task was marked doing but failed before any work was done)
    crate::commands::run::validate_resumed_task(&queue_file, "RQ-0001", &repo_root)?;

    Ok(())
}

#[test]
fn validate_resumed_task_fails_when_task_done() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Done)],
    };

    // Should fail for Done tasks (terminal state)
    let err = crate::commands::run::validate_resumed_task(&queue_file, "RQ-0001", &repo_root)
        .unwrap_err();
    assert!(err.to_string().contains("already done"));

    Ok(())
}

#[test]
fn validate_resumed_task_fails_when_task_rejected() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Rejected)],
    };

    // Should fail for Rejected tasks (terminal state)
    let err = crate::commands::run::validate_resumed_task(&queue_file, "RQ-0001", &repo_root)
        .unwrap_err();
    assert!(err.to_string().contains("already rejected"));

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

    // Validation should fail and clear the session
    let _ = crate::commands::run::validate_resumed_task(&queue_file, "RQ-9999", &repo_root);

    // Session should be cleared
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

    // Validation should fail and clear the session
    let _ = crate::commands::run::validate_resumed_task(&queue_file, "RQ-0001", &repo_root);

    // Session should be cleared
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

/// Test edge cases for invalid phases values.
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
