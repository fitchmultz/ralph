//! Parallel status document regression coverage.
//!
//! Responsibilities:
//! - Verify status guidance reflects retained artifacts, retryable outcomes, and cleanup drift.
//! - Exercise the document builder without invoking CLI rendering.
//!
//! Does not handle:
//! - Table output formatting.
//! - Worker orchestration or state persistence semantics beyond the status contract.
//!
//! Assumptions/invariants:
//! - Retained blocked artifacts should surface operator-facing recovery text.
//! - Terminal cleanup drift should remain non-blocking but visible.

use super::*;

use crate::commands::run::parallel::{BLOCKED_PUSH_MARKER_FILE, state::WorkerRecord};
use crate::contracts::MACHINE_PARALLEL_STATUS_VERSION;
use anyhow::Result;
use tempfile::TempDir;

fn blocked_marker_json(task_id: &str, attempt: u32, max_attempts: u32) -> String {
    serde_json::json!({
        "task_id": task_id,
        "reason": "push rejected after conflict review",
        "attempt": attempt,
        "max_attempts": max_attempts,
        "generated_at": "2026-03-22T00:00:00Z"
    })
    .to_string()
}

#[test]
fn parallel_status_describes_retained_blocked_workspace() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_path = temp.path().join(".ralph/workspaces/RQ-1001");
    std::fs::create_dir_all(workspace_path.join(".ralph/cache/parallel"))?;
    std::fs::write(
        workspace_path.join(BLOCKED_PUSH_MARKER_FILE),
        blocked_marker_json("RQ-1001", 3, 5),
    )?;

    let mut state = ParallelStateFile::new("2026-03-21T12:00:00Z", "main");
    let mut worker = WorkerRecord::new(
        "RQ-1001",
        workspace_path.clone(),
        "2026-03-21T12:00:00Z".to_string(),
    );
    worker.mark_blocked(
        "2026-03-21T12:05:00Z".to_string(),
        "push rejected after conflict review",
    );
    worker.push_attempts = 3;
    state.upsert_worker(worker);

    let document = build_parallel_status_document(temp.path(), Some(&state))?;
    assert_eq!(document.version, MACHINE_PARALLEL_STATUS_VERSION);
    assert_eq!(document.lifecycle_counts.total, 1);
    assert_eq!(document.lifecycle_counts.blocked, 1);
    assert_eq!(document.lifecycle_counts.running, 0);
    assert_eq!(document.lifecycle_counts.integrating, 0);
    assert_eq!(document.lifecycle_counts.completed, 0);
    assert_eq!(document.lifecycle_counts.failed, 0);
    assert_eq!(
        document.blocking.as_ref().map(|state| state.status),
        Some(BlockingStatus::Blocked)
    );
    assert!(
        document
            .blocking
            .as_ref()
            .is_some_and(|state| state.observed_at.is_some()),
        "parallel status blocking should record observed_at for operator timelines"
    );
    assert!(
        document
            .continuation
            .detail
            .contains("Retained for recovery:")
    );
    assert!(
        document
            .continuation
            .detail
            .contains("Operator action required:")
    );
    assert!(
        document
            .continuation
            .detail
            .contains(&workspace_path.display().to_string())
    );
    assert!(document.continuation.detail.contains("blocked marker 3/5"));
    assert!(
        document
            .continuation
            .detail
            .contains("push rejected after conflict review")
    );
    Ok(())
}

#[test]
fn parallel_status_distinguishes_success_failure_and_action_required() -> Result<()> {
    let temp = TempDir::new()?;
    let blocked_workspace = temp.path().join(".ralph/workspaces/RQ-3003");
    std::fs::create_dir_all(blocked_workspace.join(".ralph/cache/parallel"))?;
    std::fs::write(
        blocked_workspace.join(BLOCKED_PUSH_MARKER_FILE),
        blocked_marker_json("RQ-3003", 2, 5),
    )?;

    let mut state = ParallelStateFile::new("2026-03-21T12:00:00Z", "main");

    let mut completed = WorkerRecord::new(
        "RQ-3001",
        temp.path().join(".ralph/workspaces/RQ-3001"),
        "2026-03-21T12:00:00Z".to_string(),
    );
    completed.mark_completed("2026-03-21T12:10:00Z".to_string());
    state.upsert_worker(completed);

    let mut failed = WorkerRecord::new(
        "RQ-3002",
        temp.path().join(".ralph/workspaces/RQ-3002"),
        "2026-03-21T12:00:00Z".to_string(),
    );
    failed.mark_failed(
        "2026-03-21T12:08:00Z".to_string(),
        "worker exited with status: 1",
    );
    state.upsert_worker(failed);

    let mut blocked = WorkerRecord::new(
        "RQ-3003",
        blocked_workspace,
        "2026-03-21T12:00:00Z".to_string(),
    );
    blocked.mark_blocked(
        "2026-03-21T12:09:00Z".to_string(),
        "push rejected after conflict review",
    );
    blocked.push_attempts = 2;
    state.upsert_worker(blocked);

    let document = build_parallel_status_document(temp.path(), Some(&state))?;
    assert_eq!(document.version, MACHINE_PARALLEL_STATUS_VERSION);
    assert_eq!(document.lifecycle_counts.total, 3);
    assert_eq!(document.lifecycle_counts.completed, 1);
    assert_eq!(document.lifecycle_counts.failed, 1);
    assert_eq!(document.lifecycle_counts.blocked, 1);
    assert_eq!(document.lifecycle_counts.running, 0);
    assert_eq!(document.lifecycle_counts.integrating, 0);
    assert!(
        document
            .continuation
            .detail
            .contains("Integrated successfully:")
    );
    assert!(document.continuation.detail.contains("Retryable failures:"));
    assert!(
        document
            .continuation
            .detail
            .contains("Operator action required:")
    );
    Ok(())
}

#[test]
fn parallel_status_surfaces_cleanup_drift_without_active_workers() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_path = temp.path().join(".ralph/workspaces/RQ-2001");
    std::fs::create_dir_all(&workspace_path)?;

    let mut state = ParallelStateFile::new("2026-03-21T12:00:00Z", "main");
    let mut worker = WorkerRecord::new(
        "RQ-2001",
        workspace_path.clone(),
        "2026-03-21T12:00:00Z".to_string(),
    );
    worker.mark_completed("2026-03-21T12:05:00Z".to_string());
    state.upsert_worker(worker);

    let document = build_parallel_status_document(temp.path(), Some(&state))?;
    assert_eq!(document.lifecycle_counts.total, 1);
    assert_eq!(document.lifecycle_counts.completed, 1);
    assert_eq!(document.lifecycle_counts.running, 0);
    assert!(document.blocking.is_none());
    assert!(document.continuation.headline.contains("cleanup drift"));
    assert!(
        document
            .continuation
            .detail
            .contains("workspace cleanup left")
    );
    assert!(
        document
            .continuation
            .detail
            .contains(&workspace_path.display().to_string())
    );
    Ok(())
}

#[test]
fn parallel_status_lifecycle_counts_zero_without_parallel_state() -> Result<()> {
    let temp = TempDir::new()?;
    let document = build_parallel_status_document(temp.path(), None)?;
    assert_eq!(document.version, MACHINE_PARALLEL_STATUS_VERSION);
    assert_eq!(document.lifecycle_counts.total, 0);
    assert_eq!(document.lifecycle_counts.running, 0);
    assert_eq!(document.lifecycle_counts.integrating, 0);
    assert_eq!(document.lifecycle_counts.completed, 0);
    assert_eq!(document.lifecycle_counts.failed, 0);
    assert_eq!(document.lifecycle_counts.blocked, 0);
    Ok(())
}
