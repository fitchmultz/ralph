//! Tests for `query.rs` operations (finding/selecting runnable tasks).
//!
//! Purpose:
//! - Tests for `query.rs` operations (finding/selecting runnable tasks).
//!
//! Responsibilities:
//! - Validate runnable selection logic and query error context.
//! - Cover status, dependency, and target-id failure modes.
//!
//! Non-scope:
//! - End-to-end CLI command execution or queue persistence.
//! - Validation of task edit inputs.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Helpers construct tasks with predictable IDs and statuses.
//! - Queue order determines selection precedence.

use super::*;

#[test]
fn next_todo_task_uses_file_order_not_priority() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![
            task_with("RQ-0001", TaskStatus::Todo, vec![]),
            task_with("RQ-0002", TaskStatus::Todo, vec![]),
        ],
    };
    queue.tasks[0].priority = TaskPriority::Low;
    queue.tasks[1].priority = TaskPriority::Critical;

    let next = next_todo_task(&queue).expect("expected a todo task");

    assert_eq!(next.id, "RQ-0001");
}

#[test]
fn test_next_runnable_task_skips_blocked() {
    let mut blocked = task("RQ-0002");
    blocked.status = TaskStatus::Todo;
    blocked.depends_on = vec!["RQ-0003".to_string()]; // Depends on RQ-0003

    let mut blocker = task("RQ-0003");
    blocker.status = TaskStatus::Todo;

    let queue = QueueFile {
        version: 1,
        tasks: vec![blocked, blocker],
    };

    // blocked (RQ-0002) is first but blocked. blocker (RQ-0003) is second and runnable.
    // So next_runnable_task should return RQ-0003.
    let next = next_runnable_task(&queue, None).expect("should find runnable task");
    assert_eq!(next.id, "RQ-0003");
}

#[test]
fn select_runnable_task_index_prefers_doing() {
    let mut todo = task("RQ-0001");
    todo.status = TaskStatus::Todo;

    let mut doing = task("RQ-0002");
    doing.status = TaskStatus::Doing;

    let queue = QueueFile {
        version: 1,
        tasks: vec![todo, doing],
    };

    let idx = select_runnable_task_index(&queue, None, RunnableSelectionOptions::new(false, true))
        .expect("should select doing");
    assert_eq!(idx, 1);
}

#[test]
fn select_runnable_task_index_prefers_todo_over_draft() {
    let mut draft = task("RQ-0001");
    draft.status = TaskStatus::Draft;

    let mut todo = task("RQ-0002");
    todo.status = TaskStatus::Todo;

    let queue = QueueFile {
        version: 1,
        tasks: vec![draft, todo],
    };

    let idx = select_runnable_task_index(&queue, None, RunnableSelectionOptions::new(true, true))
        .expect("should select todo");
    assert_eq!(idx, 1);
}

#[test]
fn select_runnable_task_index_with_target_rejects_empty_id() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let err = select_runnable_task_index_with_target(
        &queue,
        None,
        "   ",
        "run --target",
        RunnableSelectionOptions::new(false, true),
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("operation=run --target"));
    assert!(msg.to_lowercase().contains("missing"));
}

#[test]
fn select_runnable_task_index_with_target_rejects_draft_without_flag() {
    let mut draft = task("RQ-0001");
    draft.status = TaskStatus::Draft;

    let queue = QueueFile {
        version: 1,
        tasks: vec![draft],
    };

    let err = select_runnable_task_index_with_target(
        &queue,
        None,
        "RQ-0001",
        "run --target",
        RunnableSelectionOptions::new(false, true),
    )
    .unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("include-draft"));
}

#[test]
fn select_runnable_task_index_with_target_rejects_unmet_dependencies() {
    let mut blocked = task("RQ-0001");
    blocked.status = TaskStatus::Todo;
    blocked.depends_on = vec!["RQ-0002".to_string()];

    let queue = QueueFile {
        version: 1,
        tasks: vec![blocked],
    };

    let err = select_runnable_task_index_with_target(
        &queue,
        None,
        "RQ-0001",
        "run --target",
        RunnableSelectionOptions::new(false, true),
    )
    .unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("dependencies"));
}

#[test]
fn test_next_runnable_task_returns_unblocked() {
    let mut t1 = task("RQ-0002");
    t1.status = TaskStatus::Todo;
    t1.depends_on = vec!["RQ-0001".to_string()];

    // Dependency is done in active queue (or done queue)
    let mut t_dep = task("RQ-0001");
    t_dep.status = TaskStatus::Done;
    t_dep.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let queue = QueueFile {
        version: 1,
        tasks: vec![t1],
    };
    let done_queue = QueueFile {
        version: 1,
        tasks: vec![t_dep],
    };

    let next = next_runnable_task(&queue, Some(&done_queue)).expect("should find runnable task");
    assert_eq!(next.id, "RQ-0002");
}

#[test]
fn test_next_runnable_task_skips_missing_dep() {
    let mut t1 = task("RQ-0002");
    t1.status = TaskStatus::Todo;
    t1.depends_on = vec!["RQ-9999".to_string()]; // Missing

    let queue = QueueFile {
        version: 1,
        tasks: vec![t1],
    };

    let next = next_runnable_task(&queue, None);
    assert!(next.is_none());
}

#[test]
fn test_next_runnable_task_allows_rejected_dep() {
    let mut t1 = task("RQ-0002");
    t1.status = TaskStatus::Todo;
    t1.depends_on = vec!["RQ-0001".to_string()];

    let mut t_rejected = task("RQ-0001");
    t_rejected.status = TaskStatus::Rejected;
    t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let queue = QueueFile {
        version: 1,
        tasks: vec![t1],
    };
    let done_queue = QueueFile {
        version: 1,
        tasks: vec![t_rejected],
    };

    // Policy: Rejected dependencies do NOT block dependents.
    let next = next_runnable_task(&queue, Some(&done_queue)).expect("should find runnable task");
    assert_eq!(next.id, "RQ-0002");
}
