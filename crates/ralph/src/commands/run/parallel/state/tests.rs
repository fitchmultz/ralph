//! Parallel state persistence regression coverage.
//!
//! Purpose:
//! - Parallel state persistence regression coverage.
//!
//! Responsibilities:
//! - Verify schema migration, worker lifecycle helpers, and state-file persistence.
//! - Keep the direct-push state contract stable while the runtime modules stay split.
//!
//! Non-scope:
//! - Parallel orchestration or worker spawning.
//! - Integration loop behavior beyond stored state fields.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Legacy schema loads migrate to the current canonical version.
//! - Unknown fields remain ignorable for forward-compatible reads.

use super::*;

use tempfile::TempDir;

#[test]
fn new_state_has_current_schema_version() {
    let state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");
    assert_eq!(state.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
}

#[test]
fn state_migration_v2_to_v3() -> Result<()> {
    let temp = TempDir::new()?;
    let path = temp.path().join("state.json");
    let workspace_path = crate::testsupport::path::portable_abs_path("ws");

    let v2_state = serde_json::json!({
        "schema_version": 2,
        "started_at": "2026-02-01T00:00:00Z",
        "base_branch": "main",
        "merge_method": "squash",
        "merge_when": "as_created",
        "tasks_in_flight": [{
            "task_id": "RQ-0001",
            "workspace_path": workspace_path,
            "branch": "b",
            "pid": 123
        }],
        "prs": [{"task_id": "RQ-0001", "pr_number": 5}],
        "pending_merges": [{
            "task_id": "RQ-0001",
            "pr_number": 5,
            "queued_at": "2026-02-01T00:00:00Z"
        }]
    });

    std::fs::write(&path, serde_json::to_string_pretty(&v2_state)?)?;

    let state = load_state(&path)?.expect("state");
    assert_eq!(state.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
    assert!(state.workers.is_empty());
    Ok(())
}

#[test]
fn worker_record_lifecycle_transitions() {
    let workspace_path = crate::testsupport::path::portable_abs_path("ws");
    let mut worker = WorkerRecord::new("RQ-0001", workspace_path, "2026-02-20T00:00:00Z".into());

    assert!(matches!(worker.lifecycle, WorkerLifecycle::Running));
    assert!(!worker.is_terminal());

    worker.start_integration();
    assert!(matches!(worker.lifecycle, WorkerLifecycle::Integrating));
    assert!(!worker.is_terminal());

    worker.mark_completed("2026-02-20T01:00:00Z".into());
    assert!(matches!(worker.lifecycle, WorkerLifecycle::Completed));
    assert!(worker.is_terminal());
    assert!(worker.completed_at.is_some());
}

#[test]
fn worker_record_mark_failed() {
    let workspace_path = crate::testsupport::path::portable_abs_path("ws");
    let mut worker = WorkerRecord::new("RQ-0001", workspace_path, "2026-02-20T00:00:00Z".into());

    worker.mark_failed("2026-02-20T01:00:00Z".into(), "CI failed");

    assert!(matches!(worker.lifecycle, WorkerLifecycle::Failed));
    assert!(worker.is_terminal());
    assert_eq!(worker.last_error, Some("CI failed".into()));
}

#[test]
fn worker_record_mark_blocked() {
    let workspace_path = crate::testsupport::path::portable_abs_path("ws");
    let mut worker = WorkerRecord::new("RQ-0001", workspace_path, "2026-02-20T00:00:00Z".into());

    worker.mark_blocked("2026-02-20T01:00:00Z".into(), "merge conflict");

    assert!(matches!(worker.lifecycle, WorkerLifecycle::BlockedPush));
    assert!(worker.is_terminal());
    assert_eq!(worker.last_error, Some("merge conflict".into()));
}

#[test]
fn worker_record_push_attempts() {
    let workspace_path = crate::testsupport::path::portable_abs_path("ws");
    let mut worker = WorkerRecord::new("RQ-0001", workspace_path, "2026-02-20T00:00:00Z".into());

    assert_eq!(worker.push_attempts, 0);
    worker.increment_push_attempt();
    assert_eq!(worker.push_attempts, 1);
    worker.increment_push_attempt();
    assert_eq!(worker.push_attempts, 2);
}

#[test]
fn state_upsert_worker_replaces_existing() {
    let mut state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");
    let ws1 = crate::testsupport::path::portable_abs_path("ws1");
    let ws2 = crate::testsupport::path::portable_abs_path("ws2");
    let ws1_new = crate::testsupport::path::portable_abs_path("ws1-new");

    state.upsert_worker(WorkerRecord::new("RQ-0001", ws1, "t1".into()));
    state.upsert_worker(WorkerRecord::new("RQ-0002", ws2, "t2".into()));

    let mut updated = WorkerRecord::new("RQ-0001", ws1_new.clone(), "t1-new".into());
    updated.start_integration();
    state.upsert_worker(updated);

    assert_eq!(state.workers.len(), 2);
    let w1 = state.get_worker("RQ-0001").expect("updated worker");
    assert_eq!(w1.workspace_path, ws1_new);
    assert!(matches!(w1.lifecycle, WorkerLifecycle::Integrating));
}

#[test]
fn state_remove_worker() {
    let mut state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");
    let ws1 = crate::testsupport::path::portable_abs_path("ws1");
    let ws2 = crate::testsupport::path::portable_abs_path("ws2");

    state.upsert_worker(WorkerRecord::new("RQ-0001", ws1, "t1".into()));
    state.upsert_worker(WorkerRecord::new("RQ-0002", ws2, "t2".into()));

    state.remove_worker("RQ-0001");

    assert_eq!(state.workers.len(), 1);
    assert!(state.get_worker("RQ-0001").is_none());
    assert!(state.get_worker("RQ-0002").is_some());
}

#[test]
fn state_active_worker_count() {
    let mut state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");
    let ws1 = crate::testsupport::path::portable_abs_path("ws1");
    let ws2 = crate::testsupport::path::portable_abs_path("ws2");
    let ws3 = crate::testsupport::path::portable_abs_path("ws3");

    let w1 = WorkerRecord::new("RQ-0001", ws1, "t1".into());
    let mut w2 = WorkerRecord::new("RQ-0002", ws2, "t2".into());
    let mut w3 = WorkerRecord::new("RQ-0003", ws3, "t3".into());

    w2.mark_completed("t".into());
    w3.mark_blocked("t".into(), "error");

    state.upsert_worker(w1);
    state.upsert_worker(w2);
    state.upsert_worker(w3);

    assert_eq!(state.active_worker_count(), 1);
}

#[test]
fn state_round_trips() -> Result<()> {
    let temp = TempDir::new()?;
    let path = temp.path().join("state.json");
    let workspace_path = crate::testsupport::path::portable_abs_path("ws");

    let mut state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");
    let mut worker = WorkerRecord::new(
        "RQ-0001",
        workspace_path.clone(),
        "2026-02-20T00:00:00Z".into(),
    );
    worker.start_integration();
    worker.increment_push_attempt();
    state.upsert_worker(worker);

    save_state(&path, &state)?;
    let loaded = load_state(&path)?.expect("state");

    assert_eq!(loaded.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
    assert_eq!(loaded.target_branch, "main");
    assert_eq!(loaded.workers.len(), 1);

    let worker = &loaded.workers[0];
    assert_eq!(worker.task_id, "RQ-0001");
    assert_eq!(worker.workspace_path, workspace_path);
    assert!(matches!(worker.lifecycle, WorkerLifecycle::Integrating));
    assert_eq!(worker.push_attempts, 1);

    Ok(())
}

#[test]
fn state_deserialization_ignores_unknown_fields() -> Result<()> {
    let raw = serde_json::json!({
        "schema_version": 3,
        "started_at": "2026-02-20T00:00:00Z",
        "target_branch": "main",
        "unknown_top": "ignored",
        "workers": [{
            "task_id": "RQ-0001",
            "workspace_path": crate::testsupport::path::portable_abs_path("ws"),
            "started_at": "2026-02-20T00:00:00Z",
            "unknown_worker": "ignored"
        }]
    });

    let state: ParallelStateFile = serde_json::from_value(raw)?;
    assert_eq!(state.workers.len(), 1);
    assert_eq!(state.workers[0].task_id, "RQ-0001");
    Ok(())
}
