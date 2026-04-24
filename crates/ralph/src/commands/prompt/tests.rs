//! Tests for prompt command helpers.
//!
//! Purpose:
//! - Tests for prompt command helpers.
//!
//! Responsibilities:
//! - Validate worker task-id resolution behavior used by prompt previews.
//! - Keep prompt helper tests adjacent to the helper seams they cover.
//!
//! Not handled here:
//! - Full prompt rendering integration tests (see `crates/ralph/tests/prompt_cmd_test.rs`).
//! - CLI parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Temp directories stand in for isolated repo roots.

use tempfile::TempDir;

use crate::config::Resolved;
use crate::contracts::{Config, QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue;

use super::worker::resolve_worker_task_id;

fn make_task(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        title: format!("Task {id}"),
        description: None,
        status,
        priority: TaskPriority::Medium,
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test".to_string()],
        plan: vec!["plan".to_string()],
        notes: vec![],
        request: Some("request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    }
}

fn make_resolved(temp: &TempDir) -> Resolved {
    let repo_root = temp.path().to_path_buf();
    let queue_path = repo_root.join("queue.json");
    let done_path = repo_root.join("done.json");
    Resolved {
        config: Config::default(),
        repo_root,
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

#[test]
fn resolve_worker_task_id_trims_explicit_task_id() {
    let temp = TempDir::new().expect("tempdir");
    let resolved = make_resolved(&temp);
    let id =
        resolve_worker_task_id(&resolved, Some("  RQ-0009  ".to_string())).expect("should trim");
    assert_eq!(id, "RQ-0009");
}

#[test]
fn resolve_worker_task_id_prefers_doing() {
    let temp = TempDir::new().expect("tempdir");
    let resolved = make_resolved(&temp);
    let queue_file = QueueFile {
        version: 1,
        tasks: vec![
            make_task("RQ-0001", TaskStatus::Todo),
            make_task("RQ-0002", TaskStatus::Doing),
        ],
    };
    queue::save_queue(&resolved.queue_path, &queue_file).expect("save queue");

    let id = resolve_worker_task_id(&resolved, None).expect("should resolve doing");
    assert_eq!(id, "RQ-0002");
}

#[test]
fn resolve_worker_task_id_returns_runnable_todo() {
    let temp = TempDir::new().expect("tempdir");
    let resolved = make_resolved(&temp);

    let mut todo = make_task("RQ-0003", TaskStatus::Todo);
    todo.depends_on = vec!["RQ-0002".to_string()];

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![todo],
    };
    let done_file = QueueFile {
        version: 1,
        tasks: vec![make_task("RQ-0002", TaskStatus::Done)],
    };
    queue::save_queue(&resolved.queue_path, &queue_file).expect("save queue");
    queue::save_queue(&resolved.done_path, &done_file).expect("save done");

    let id = resolve_worker_task_id(&resolved, None).expect("should resolve todo");
    assert_eq!(id, "RQ-0003");
}
