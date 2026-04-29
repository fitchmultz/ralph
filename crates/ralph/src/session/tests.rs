//! Session module tests.
//!
//! Purpose:
//! - Session module tests.
//!
//! Responsibilities:
//! - Verify persistence, validation, progress tracking, and non-interactive recovery behavior.
//!
//! Not handled here:
//! - Full run-loop integration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Time-sensitive validation uses fixed timestamps where practical.

use super::*;
use crate::contracts::{QueueFile, SessionState, Task, TaskPriority, TaskStatus};
use crate::testsupport::git as git_test;
use crate::timeutil;
use tempfile::TempDir;
use time::Duration;

fn test_task(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test".to_string(),
        description: None,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: Default::default(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    }
}

const TEST_NOW: &str = "2026-02-07T12:00:00.000000000Z";

fn test_now() -> time::OffsetDateTime {
    timeutil::parse_rfc3339(TEST_NOW).unwrap()
}

fn test_session_with_time(task_id: &str, last_updated_at: &str) -> SessionState {
    SessionState::new(
        "test-session-id".to_string(),
        task_id.to_string(),
        last_updated_at.to_string(),
        1,
        crate::contracts::Runner::Claude,
        "sonnet".to_string(),
        0,
        None,
        None,
    )
}

fn test_session(task_id: &str) -> SessionState {
    test_session_with_time(task_id, TEST_NOW)
}

fn empty_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![],
    }
}

#[test]
fn get_git_head_commit_returns_current_head() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    git_test::init_repo(temp_dir.path())?;
    std::fs::write(temp_dir.path().join("README.md"), "session commit")?;
    git_test::commit_all(temp_dir.path(), "init")?;

    let commit = get_git_head_commit(temp_dir.path());
    let expected = git_test::git_output(temp_dir.path(), &["rev-parse", "HEAD"])?;

    assert_eq!(commit.as_deref(), Some(expected.as_str()));
    Ok(())
}

#[test]
fn save_and_load_session_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let session = test_session("RQ-0001");

    save_session(temp_dir.path(), &session).unwrap();
    let loaded = load_session(temp_dir.path()).unwrap().unwrap();

    assert_eq!(loaded.session_id, session.session_id);
    assert_eq!(loaded.task_id, session.task_id);
    assert_eq!(loaded.iterations_planned, session.iterations_planned);
}

#[test]
fn clear_session_removes_file() {
    let temp_dir = TempDir::new().unwrap();
    let session = test_session("RQ-0001");

    save_session(temp_dir.path(), &session).unwrap();
    assert!(session_exists(temp_dir.path()));

    clear_session(temp_dir.path()).unwrap();
    assert!(!session_exists(temp_dir.path()));
}

#[test]
fn validate_session_valid_when_task_doing() {
    let session = test_session("RQ-0001");
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    assert!(matches!(
        validate_session(&session, &queue, None),
        SessionValidationResult::Valid(_)
    ));
}

#[test]
fn validate_session_stale_when_task_not_doing() {
    let session = test_session("RQ-0001");
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Todo)],
    };

    assert!(matches!(
        validate_session(&session, &queue, None),
        SessionValidationResult::Stale { .. }
    ));
}

#[test]
fn validate_session_stale_when_task_missing() {
    let session = test_session("RQ-0001");
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0002", TaskStatus::Doing)],
    };

    assert!(matches!(
        validate_session(&session, &queue, None),
        SessionValidationResult::Stale { .. }
    ));
}

#[test]
fn check_session_returns_no_session_when_file_missing() {
    let temp_dir = TempDir::new().unwrap();
    let queue = empty_queue();

    assert_eq!(
        check_session(temp_dir.path(), &queue, None).unwrap(),
        SessionValidationResult::NoSession
    );
}

#[test]
fn check_session_classifies_malformed_json_as_corrupt_cache() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(session_path(&cache_dir), "{ definitely not valid json").unwrap();
    let queue = empty_queue();

    match check_session(&cache_dir, &queue, Some(24)).unwrap() {
        SessionValidationResult::CorruptCache(corruption) => {
            assert_eq!(corruption.path, session_path(&cache_dir));
            assert!(corruption.diagnostic.contains("parse session file"));
            assert!(!corruption.diagnostic.contains("definitely not valid json"));
        }
        other => panic!("expected corrupt cache, got {other:?}"),
    }
}

#[test]
fn check_session_classifies_session_path_directory_as_corrupt_cache() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(session_path(&cache_dir)).unwrap();
    let queue = empty_queue();

    match check_session(&cache_dir, &queue, Some(24)).unwrap() {
        SessionValidationResult::CorruptCache(corruption) => {
            assert_eq!(corruption.path, session_path(&cache_dir));
            assert!(corruption.diagnostic.contains("read session file"));
        }
        other => panic!("expected corrupt cache, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn check_session_classifies_uninspectable_session_path_as_corrupt_cache() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(session_path(&cache_dir), "{}").unwrap();

    let original_mode = std::fs::metadata(&cache_dir).unwrap().permissions().mode();
    let mut locked_permissions = std::fs::metadata(&cache_dir).unwrap().permissions();
    locked_permissions.set_mode(0o000);
    std::fs::set_permissions(&cache_dir, locked_permissions).unwrap();

    let result = check_session(&cache_dir, &empty_queue(), Some(24));

    let mut restored_permissions = std::fs::metadata(temp_dir.path().join("cache"))
        .unwrap()
        .permissions();
    restored_permissions.set_mode(original_mode);
    std::fs::set_permissions(&cache_dir, restored_permissions).unwrap();

    match result.unwrap() {
        SessionValidationResult::CorruptCache(corruption) => {
            assert_eq!(corruption.path, session_path(&cache_dir));
            assert!(corruption.diagnostic.contains("inspect session file"));
        }
        other => panic!("expected corrupt cache, got {other:?}"),
    }
}

#[test]
fn resolve_run_session_decision_corrupt_json_falls_back_fresh_and_quarantines() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(session_path(&cache_dir), "{ definitely not valid json").unwrap();
    let queue = empty_queue();

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::AutoResume,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Execute,
        },
    )
    .unwrap();

    assert_eq!(resolution.resume_task_id, None);
    let decision = resolution.decision.expect("decision");
    assert_eq!(decision.status, ResumeStatus::FallingBackToFreshInvocation);
    assert_eq!(decision.reason, ResumeReason::SessionCacheCorrupt);
    assert!(decision.blocking_state().is_none());
    assert!(!session_exists(&cache_dir));
    let quarantine_dir = cache_dir.join("session-quarantine");
    assert!(quarantine_dir.exists());
    assert_eq!(std::fs::read_dir(quarantine_dir).unwrap().count(), 1);
}

#[test]
fn resolve_run_session_decision_corrupt_json_preview_refuses_and_preserves_cache() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let original_path = session_path(&cache_dir);
    std::fs::write(&original_path, "not-json").unwrap();
    let queue = empty_queue();

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::Prompt,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Preview,
        },
    )
    .unwrap();

    let decision = resolution.decision.expect("decision");
    assert_eq!(decision.status, ResumeStatus::RefusingToResume);
    assert_eq!(decision.reason, ResumeReason::SessionCacheCorrupt);
    assert!(original_path.exists());
    assert!(!cache_dir.join("session-quarantine").exists());
}

#[test]
fn resolve_run_session_decision_corrupt_json_prompt_execute_refuses_and_quarantines() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(session_path(&cache_dir), "not-json").unwrap();
    let queue = empty_queue();

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::Prompt,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Execute,
        },
    )
    .unwrap();

    let decision = resolution.decision.expect("decision");
    assert_eq!(decision.status, ResumeStatus::RefusingToResume);
    assert_eq!(decision.reason, ResumeReason::SessionCacheCorrupt);
    assert!(!session_exists(&cache_dir));
    assert!(cache_dir.join("session-quarantine").exists());
}

#[test]
fn resume_decision_blocking_state_for_corrupt_cache() {
    let decision = ResumeDecision {
        status: ResumeStatus::RefusingToResume,
        scope: ResumeScope::RunSession,
        reason: ResumeReason::SessionCacheCorrupt,
        task_id: None,
        message:
            "Resume: refusing to guess because the saved session cache is corrupt or unreadable."
                .to_string(),
        detail: "Inspect .ralph/cache/session.jsonc.".to_string(),
    };

    let blocking = decision.blocking_state().expect("blocking state");
    assert!(matches!(
        blocking.reason,
        crate::contracts::BlockingReason::RunnerRecovery { ref reason, .. }
            if reason == "session_cache_corrupt"
    ));
}

#[test]
fn session_path_returns_correct_path() {
    let temp_dir = TempDir::new().unwrap();
    assert_eq!(
        session_path(temp_dir.path()),
        temp_dir.path().join("session.jsonc")
    );
}

#[test]
fn prompt_session_recovery_returns_false_when_non_interactive() {
    let session = test_session("RQ-0001");
    assert!(!prompt_session_recovery(&session, true).unwrap());
}

#[test]
fn prompt_session_recovery_timeout_returns_false_when_non_interactive() {
    let session = test_session("RQ-0001");
    assert!(!prompt_session_recovery_timeout(&session, 48, 24, true).unwrap());
}

#[test]
fn validate_session_returns_timeout_when_older_than_threshold() {
    let now = test_now();
    let session_time = now - Duration::hours(48);
    let session =
        test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    match validate_session_with_now(&session, &queue, Some(24), now) {
        SessionValidationResult::Timeout {
            hours,
            session: timed_out,
        } => {
            assert_eq!(hours, 48);
            assert_eq!(timed_out.task_id, session.task_id);
            assert_eq!(timed_out.session_id, session.session_id);
        }
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[test]
fn check_session_returns_timeout_and_includes_loaded_session() {
    let temp_dir = TempDir::new().unwrap();
    let session_time = time::OffsetDateTime::now_utc() - Duration::days(365);
    let session =
        test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
    save_session(temp_dir.path(), &session).unwrap();

    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    match check_session(temp_dir.path(), &queue, Some(24)).unwrap() {
        SessionValidationResult::Timeout {
            hours,
            session: timed_out,
        } => {
            assert!(hours >= 24);
            assert_eq!(timed_out.task_id, session.task_id);
            assert_eq!(timed_out.session_id, session.session_id);
            assert_eq!(timed_out.last_updated_at, session.last_updated_at);
        }
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[test]
fn validate_session_returns_valid_when_within_custom_threshold() {
    let now = test_now();
    let session_time = now - Duration::hours(12);
    let session =
        test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    assert!(matches!(
        validate_session_with_now(&session, &queue, Some(48), now),
        SessionValidationResult::Valid(_)
    ));
}

#[test]
fn validate_session_returns_valid_when_within_default_threshold() {
    let now = test_now();
    let session_time = now - Duration::hours(1);
    let session =
        test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    assert!(matches!(
        validate_session_with_now(&session, &queue, Some(24), now),
        SessionValidationResult::Valid(_)
    ));
}

#[test]
fn validate_session_returns_valid_when_no_timeout_configured() {
    let session = test_session("RQ-0001");
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    assert!(matches!(
        validate_session(&session, &queue, None),
        SessionValidationResult::Valid(_)
    ));
}

#[test]
fn validate_session_invalid_last_updated_does_not_timeout() {
    let session = test_session_with_time("RQ-0001", "not-a-valid-timestamp");
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    assert!(matches!(
        validate_session_with_now(&session, &queue, Some(1), test_now()),
        SessionValidationResult::Valid(_)
    ));
}

#[test]
fn validate_session_exact_boundary_returns_timeout() {
    let now = test_now();
    let session_time = now - Duration::hours(24);
    let session =
        test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    assert!(matches!(
        validate_session_with_now(&session, &queue, Some(24), now),
        SessionValidationResult::Timeout { .. }
    ));
}

#[test]
fn validate_session_future_timestamp_no_timeout() {
    let now = test_now();
    let session_time = now + Duration::hours(1);
    let session =
        test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    assert!(matches!(
        validate_session_with_now(&session, &queue, Some(1), now),
        SessionValidationResult::Valid(_)
    ));
}

#[test]
fn resolve_run_session_decision_auto_resume_resumes_valid_session() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let mut session = test_session_with_time("RQ-0001", &timeutil::now_utc_rfc3339_or_fallback());
    session.current_phase = 2;
    session.tasks_completed_in_loop = 3;
    save_session(&cache_dir, &session).unwrap();

    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::AutoResume,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Execute,
        },
    )
    .unwrap();

    assert_eq!(resolution.resume_task_id.as_deref(), Some("RQ-0001"));
    assert_eq!(resolution.completed_count, 3);
    let decision = resolution.decision.expect("decision present");
    assert_eq!(decision.status, ResumeStatus::ResumingSameSession);
    assert_eq!(decision.reason, ResumeReason::SessionValid);
}

#[test]
fn resolve_run_session_decision_marks_stale_session_as_fresh_start() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let session = test_session("RQ-0001");
    save_session(&cache_dir, &session).unwrap();

    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Done)],
    };

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::AutoResume,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Execute,
        },
    )
    .unwrap();

    assert_eq!(resolution.resume_task_id, None);
    let decision = resolution.decision.expect("decision present");
    assert_eq!(decision.status, ResumeStatus::FallingBackToFreshInvocation);
    assert_eq!(decision.reason, ResumeReason::SessionStale);
    assert!(!session_exists(&cache_dir));
}

#[test]
fn resolve_run_session_decision_hides_missing_session_when_not_requested() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let queue = empty_queue();

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::AutoResume,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Execute,
        },
    )
    .unwrap();

    assert_eq!(resolution.resume_task_id, None);
    assert!(resolution.decision.is_none());
}

#[test]
fn resolve_run_session_decision_preview_stale_session_preserves_cache() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let session = test_session("RQ-0001");
    save_session(&cache_dir, &session).unwrap();

    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Done)],
    };

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::AutoResume,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Preview,
        },
    )
    .unwrap();

    assert_eq!(resolution.resume_task_id, None);
    assert_eq!(
        resolution.decision.expect("decision").reason,
        ResumeReason::SessionStale
    );
    assert!(session_exists(&cache_dir));
}

#[test]
fn resolve_run_session_decision_timed_out_noninteractive_refusal_keeps_cache() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let stale_time = time::OffsetDateTime::now_utc() - Duration::hours(72);
    let session = test_session_with_time("RQ-0001", &timeutil::format_rfc3339(stale_time).unwrap());
    save_session(&cache_dir, &session).unwrap();

    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::Prompt,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Execute,
        },
    )
    .unwrap();

    assert_eq!(resolution.resume_task_id, None);
    let decision = resolution.decision.expect("decision");
    assert_eq!(decision.status, ResumeStatus::RefusingToResume);
    assert_eq!(
        decision.reason,
        ResumeReason::SessionTimedOutRequiresConfirmation
    );
    assert!(session_exists(&cache_dir));
}

#[test]
fn resolve_run_session_decision_refuses_prompt_required_noninteractive_resume() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let session = test_session_with_time("RQ-0001", &timeutil::now_utc_rfc3339_or_fallback());
    save_session(&cache_dir, &session).unwrap();

    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
    };

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::Prompt,
            non_interactive: true,
            explicit_task_id: None,
            announce_missing_session: false,
            mode: ResumeDecisionMode::Execute,
        },
    )
    .unwrap();

    assert_eq!(resolution.resume_task_id, None);
    let decision = resolution.decision.expect("decision present");
    assert_eq!(decision.status, ResumeStatus::RefusingToResume);
    assert_eq!(decision.reason, ResumeReason::ResumeConfirmationRequired);
    assert!(session_exists(&cache_dir));
}

#[test]
fn resolve_run_session_decision_explicit_task_overrides_unrelated_session() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let session = test_session_with_time("RQ-0001", &timeutil::now_utc_rfc3339_or_fallback());
    save_session(&cache_dir, &session).unwrap();

    let queue = QueueFile {
        version: 1,
        tasks: vec![
            test_task("RQ-0001", TaskStatus::Doing),
            test_task("RQ-0002", TaskStatus::Todo),
        ],
    };

    let resolution = resolve_run_session_decision(
        &cache_dir,
        &queue,
        RunSessionDecisionOptions {
            timeout_hours: Some(24),
            behavior: ResumeBehavior::AutoResume,
            non_interactive: true,
            explicit_task_id: Some("RQ-0002"),
            announce_missing_session: false,
            mode: ResumeDecisionMode::Execute,
        },
    )
    .unwrap();

    assert_eq!(resolution.resume_task_id, None);
    let decision = resolution.decision.expect("decision present");
    assert_eq!(
        decision.reason,
        ResumeReason::ExplicitTaskSelectionOverridesSession
    );
}

#[test]
fn increment_session_progress_updates_and_persists() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let session = test_session("RQ-0001");
    save_session(&cache_dir, &session).unwrap();
    assert_eq!(session.tasks_completed_in_loop, 0);

    increment_session_progress(&cache_dir).unwrap();
    let loaded = load_session(&cache_dir).unwrap().unwrap();
    assert_eq!(loaded.tasks_completed_in_loop, 1);

    increment_session_progress(&cache_dir).unwrap();
    let loaded = load_session(&cache_dir).unwrap().unwrap();
    assert_eq!(loaded.tasks_completed_in_loop, 2);
}

#[test]
fn increment_session_progress_handles_missing_session() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    increment_session_progress(&cache_dir).unwrap();
}

#[test]
fn resume_decision_blocking_state_for_confirmation_required() {
    let decision = ResumeDecision {
        status: ResumeStatus::RefusingToResume,
        scope: ResumeScope::RunSession,
        reason: ResumeReason::ResumeConfirmationRequired,
        task_id: Some("RQ-0007".to_string()),
        message: "Resume: refusing to guess.".to_string(),
        detail: "Confirmation is unavailable.".to_string(),
    };

    let blocking = decision.blocking_state().expect("blocking state");
    assert_eq!(blocking.task_id.as_deref(), Some("RQ-0007"));
    assert!(matches!(
        blocking.reason,
        crate::contracts::BlockingReason::RunnerRecovery { .. }
    ));
}

#[test]
fn resume_decision_without_recovery_blocker_has_no_blocking_state() {
    let decision = ResumeDecision {
        status: ResumeStatus::FallingBackToFreshInvocation,
        scope: ResumeScope::RunSession,
        reason: ResumeReason::SessionStale,
        task_id: None,
        message: "Resume: starting fresh because the saved session is stale.".to_string(),
        detail: "The session no longer matches the queue.".to_string(),
    };

    assert!(decision.blocking_state().is_none());
}
