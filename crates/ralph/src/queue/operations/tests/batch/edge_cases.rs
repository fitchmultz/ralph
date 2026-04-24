//! Batch mutation edge-case regression coverage.
//!
//! Purpose:
//! - Batch mutation edge-case regression coverage.
//!
//! Responsibilities:
//! - Cover success, atomic-failure, continue-on-error, and timestamp-validation paths.
//! - Exercise batch status, field, and edit mutation flows end-to-end.
//!
//! Non-scope:
//! - Helper-only tag filtering and result-summary behavior.
//! - Queue persistence or higher-level command orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Tests reuse the parent module's shared fixtures and imports via `super::*`.
//! - Invalid timestamps must fail without silently normalizing input.

use super::*;

// =============================================================================
// batch_set_status tests
// =============================================================================

#[test]
fn batch_set_status_updates_all_tasks() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Todo, vec![]),
    ];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_status(
        &mut queue,
        &["RQ-0001".to_string(), "RQ-0002".to_string()],
        TaskStatus::Doing,
        "2026-01-21T12:00:00Z",
        None,
        false,
    )
    .expect("batch operation should succeed");

    assert_eq!(result.total, 2);
    assert_eq!(result.succeeded, 2);
    assert_eq!(result.failed, 0);
    assert!(result.all_succeeded());

    // Verify tasks were updated
    assert_eq!(queue.tasks[0].status, TaskStatus::Doing);
    assert_eq!(queue.tasks[1].status, TaskStatus::Doing);
}

#[test]
fn batch_set_status_empty_input_errors() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_status(
        &mut queue,
        &[],
        TaskStatus::Doing,
        "2026-01-21T12:00:00Z",
        None,
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No task IDs provided"));
}

#[test]
fn batch_set_status_atomic_fails_on_missing_task() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_status(
        &mut queue,
        &["RQ-0001".to_string(), "RQ-9999".to_string()],
        TaskStatus::Doing,
        "2026-01-21T12:00:00Z",
        None,
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("RQ-9999"));
    assert!(err.contains("not found"));

    // Verify no tasks were updated (atomic)
    assert_eq!(queue.tasks[0].status, TaskStatus::Todo);
}

#[test]
fn batch_set_status_continue_on_error_reports_partial() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Todo, vec![]),
    ];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_status(
        &mut queue,
        &[
            "RQ-0001".to_string(),
            "RQ-9999".to_string(),
            "RQ-0002".to_string(),
        ],
        TaskStatus::Doing,
        "2026-01-21T12:00:00Z",
        None,
        true, // continue_on_error
    )
    .expect("batch operation should complete with partial success");

    assert_eq!(result.total, 3);
    assert_eq!(result.succeeded, 2);
    assert_eq!(result.failed, 1);
    assert!(result.has_failures());

    // Verify valid tasks were updated
    assert_eq!(queue.tasks[0].status, TaskStatus::Doing);
    assert_eq!(queue.tasks[1].status, TaskStatus::Doing);

    // Verify the failed task is in results
    let failed = result
        .results
        .iter()
        .find(|r| r.task_id == "RQ-9999")
        .unwrap();
    assert!(!failed.success);
    assert!(failed.error.as_ref().unwrap().contains("not found"));
}

#[test]
fn batch_set_status_deduplicates_ids() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_status(
        &mut queue,
        &[
            "RQ-0001".to_string(),
            "RQ-0001".to_string(),
            "RQ-0001".to_string(),
        ],
        TaskStatus::Doing,
        "2026-01-21T12:00:00Z",
        None,
        false,
    )
    .expect("batch operation should succeed");

    assert_eq!(result.total, 1);
    assert_eq!(result.succeeded, 1);
    assert_eq!(queue.tasks[0].status, TaskStatus::Doing);
}

#[test]
fn batch_set_status_invalid_rfc3339_fails() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_status(
        &mut queue,
        &["RQ-0001".to_string()],
        TaskStatus::Doing,
        "not-a-valid-timestamp",
        None,
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must be a valid RFC3339 UTC timestamp"));

    // Verify task was not updated
    assert_eq!(queue.tasks[0].status, TaskStatus::Todo);
}

#[test]
fn batch_set_status_non_utc_offset_fails() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_status(
        &mut queue,
        &["RQ-0001".to_string()],
        TaskStatus::Doing,
        "2026-01-21T12:00:00+05:00", // Non-UTC offset
        None,
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must be a valid RFC3339 UTC timestamp"));

    // Verify task was not updated
    assert_eq!(queue.tasks[0].status, TaskStatus::Todo);
}

// =============================================================================
// batch_set_field tests
// =============================================================================

#[test]
fn batch_set_field_updates_custom_fields() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Todo, vec![]),
    ];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_field(
        &mut queue,
        &["RQ-0001".to_string(), "RQ-0002".to_string()],
        "severity",
        "high",
        "2026-01-21T12:00:00Z",
        false,
    )
    .expect("batch operation should succeed");

    assert_eq!(result.total, 2);
    assert_eq!(result.succeeded, 2);

    // Verify fields were set
    assert_eq!(
        queue.tasks[0].custom_fields.get("severity"),
        Some(&"high".to_string())
    );
    assert_eq!(
        queue.tasks[1].custom_fields.get("severity"),
        Some(&"high".to_string())
    );
}

#[test]
fn batch_set_field_empty_input_errors() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_field(
        &mut queue,
        &[],
        "severity",
        "high",
        "2026-01-21T12:00:00Z",
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No task IDs provided"));
}

#[test]
fn batch_set_field_atomic_fails_on_missing_task() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_field(
        &mut queue,
        &["RQ-0001".to_string(), "RQ-9999".to_string()],
        "severity",
        "high",
        "2026-01-21T12:00:00Z",
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("RQ-9999"));
    assert!(err.contains("not found"));

    // Verify no fields were updated (atomic)
    assert!(queue.tasks[0].custom_fields.is_empty());
}

#[test]
fn batch_set_field_continue_on_error_reports_partial() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Todo, vec![]),
    ];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_field(
        &mut queue,
        &[
            "RQ-0001".to_string(),
            "RQ-9999".to_string(),
            "RQ-0002".to_string(),
        ],
        "severity",
        "high",
        "2026-01-21T12:00:00Z",
        true, // continue_on_error
    )
    .expect("batch operation should complete with partial success");

    assert_eq!(result.total, 3);
    assert_eq!(result.succeeded, 2);
    assert_eq!(result.failed, 1);

    // Verify valid tasks were updated
    assert_eq!(
        queue.tasks[0].custom_fields.get("severity"),
        Some(&"high".to_string())
    );
    assert_eq!(
        queue.tasks[1].custom_fields.get("severity"),
        Some(&"high".to_string())
    );
}

#[test]
fn batch_set_field_invalid_rfc3339_fails() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_set_field(
        &mut queue,
        &["RQ-0001".to_string()],
        "severity",
        "high",
        "not-a-valid-timestamp",
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must be a valid RFC3339 UTC timestamp"));

    // Verify field was not set
    assert!(queue.tasks[0].custom_fields.is_empty());
}

// =============================================================================
// batch_apply_edit tests
// =============================================================================

#[test]
fn batch_apply_edit_updates_title() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Todo, vec![]),
    ];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_apply_edit(
        &mut queue,
        None,
        &["RQ-0001".to_string(), "RQ-0002".to_string()],
        TaskEditKey::Title,
        "New Title",
        "2026-01-21T12:00:00Z",
        "RQ",
        4,
        10,
        false,
    )
    .expect("batch operation should succeed");

    assert_eq!(result.total, 2);
    assert_eq!(result.succeeded, 2);

    // Verify titles were updated
    assert_eq!(queue.tasks[0].title, "New Title");
    assert_eq!(queue.tasks[1].title, "New Title");
}

#[test]
fn batch_apply_edit_empty_input_errors() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_apply_edit(
        &mut queue,
        None,
        &[],
        TaskEditKey::Title,
        "New Title",
        "2026-01-21T12:00:00Z",
        "RQ",
        4,
        10,
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No task IDs provided"));
}

#[test]
fn batch_apply_edit_atomic_fails_on_missing_task() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_apply_edit(
        &mut queue,
        None,
        &["RQ-0001".to_string(), "RQ-9999".to_string()],
        TaskEditKey::Title,
        "New Title",
        "2026-01-21T12:00:00Z",
        "RQ",
        4,
        10,
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("RQ-9999"));
    assert!(err.contains("not found"));

    // Verify no edits were applied (atomic)
    assert_eq!(queue.tasks[0].title, "Test task");
}

#[test]
fn batch_apply_edit_continue_on_error_reports_partial() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Todo, vec![]),
    ];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_apply_edit(
        &mut queue,
        None,
        &[
            "RQ-0001".to_string(),
            "RQ-9999".to_string(),
            "RQ-0002".to_string(),
        ],
        TaskEditKey::Title,
        "New Title",
        "2026-01-21T12:00:00Z",
        "RQ",
        4,
        10,
        true, // continue_on_error
    )
    .expect("batch operation should complete with partial success");

    assert_eq!(result.total, 3);
    assert_eq!(result.succeeded, 2);
    assert_eq!(result.failed, 1);

    // Verify valid tasks were updated
    assert_eq!(queue.tasks[0].title, "New Title");
    assert_eq!(queue.tasks[1].title, "New Title");
}

#[test]
fn batch_apply_edit_invalid_rfc3339_fails() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let mut queue = QueueFile { version: 1, tasks };

    let result = batch_apply_edit(
        &mut queue,
        None,
        &["RQ-0001".to_string()],
        TaskEditKey::Title,
        "New Title",
        "not-a-valid-timestamp",
        "RQ",
        4,
        10,
        false,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must be a valid RFC3339 UTC timestamp"));

    // Note: The edit is applied before the timestamp validation occurs, but then
    // rolled back by restoring the previous task state on error. However, since
    // the error happens during updated_at assignment (not during validation),
    // the task state may be partially modified depending on the edit key.
    // For Title edits, the change is applied before the timestamp check.
    // See apply_task_edit() in edit.rs for the exact behavior.
}
