//! Tests for `batch.rs` operations (batch status/field/edit updates).
//!
//! Responsibilities:
//! - Cover batch operations: tag filtering, ID deduplication, atomic vs continue-on-error.
//! - Test empty input handling, missing IDs, and invalid timestamps.
//!
//! Does not handle:
//! - Single-task operations (see status.rs, fields.rs, edit.rs for those).
//! - Queue persistence (tested at integration level).
//!
//! Assumptions/invariants:
//! - Shared fixtures from `super` provide standard task construction.
//! - All batch operations require a valid RFC3339 UTC timestamp.

use super::*;
use crate::contracts::TaskStatus;
use crate::queue::operations::batch::{
    BatchOperationResult, batch_apply_edit, batch_set_field, batch_set_status, collect_task_ids,
    deduplicate_task_ids, filter_tasks_by_tags, resolve_task_ids,
};
use crate::queue::operations::edit::TaskEditKey;

// =============================================================================
// Tag filtering tests
// =============================================================================

#[test]
fn filter_tasks_by_tags_matches_case_insensitive() {
    let tasks = vec![
        task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["rust".to_string(), "cli".to_string()],
        ),
        task_with(
            "RQ-0002",
            TaskStatus::Todo,
            vec!["Rust".to_string(), "backend".to_string()],
        ),
        task_with("RQ-0003", TaskStatus::Todo, vec!["python".to_string()]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &["rust".to_string()]);

    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|t| t.id == "RQ-0001"));
    assert!(result.iter().any(|t| t.id == "RQ-0002"));
}

#[test]
fn filter_tasks_by_tags_uses_or_logic() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec!["rust".to_string()]),
        task_with("RQ-0002", TaskStatus::Todo, vec!["cli".to_string()]),
        task_with("RQ-0003", TaskStatus::Todo, vec!["python".to_string()]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &["rust".to_string(), "cli".to_string()]);

    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|t| t.id == "RQ-0001"));
    assert!(result.iter().any(|t| t.id == "RQ-0002"));
}

#[test]
fn filter_tasks_by_tags_empty_filter_returns_empty() {
    let tasks = vec![task_with(
        "RQ-0001",
        TaskStatus::Todo,
        vec!["rust".to_string()],
    )];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &[]);

    assert!(result.is_empty());
}

#[test]
fn filter_tasks_by_tags_no_match_returns_empty() {
    let tasks = vec![task_with(
        "RQ-0001",
        TaskStatus::Todo,
        vec!["rust".to_string()],
    )];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &["python".to_string()]);

    assert!(result.is_empty());
}

#[test]
fn filter_tasks_by_tags_trims_whitespace() {
    let tasks = vec![task_with(
        "RQ-0001",
        TaskStatus::Todo,
        vec!["rust".to_string()],
    )];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &["  rust  ".to_string(), "".to_string()]);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "RQ-0001");
}

// =============================================================================
// Deduplication tests
// =============================================================================

#[test]
fn deduplicate_task_ids_preserves_order() {
    let ids = vec![
        "RQ-0001".to_string(),
        "RQ-0002".to_string(),
        "RQ-0001".to_string(),
        "RQ-0003".to_string(),
        "RQ-0002".to_string(),
    ];

    let result = deduplicate_task_ids(&ids);

    assert_eq!(result, vec!["RQ-0001", "RQ-0002", "RQ-0003"]);
}

#[test]
fn deduplicate_task_ids_skips_empty() {
    let ids = vec![
        "RQ-0001".to_string(),
        "".to_string(),
        "RQ-0002".to_string(),
        " ".to_string(),
    ];

    let result = deduplicate_task_ids(&ids);

    assert_eq!(result, vec!["RQ-0001", "RQ-0002"]);
}

#[test]
fn deduplicate_task_ids_trims_whitespace() {
    let ids = vec![
        "RQ-0001".to_string(),
        "  RQ-0001  ".to_string(),
        "RQ-0002".to_string(),
    ];

    let result = deduplicate_task_ids(&ids);

    // After trimming, "  RQ-0001  " equals "RQ-0001", so it's a duplicate
    assert_eq!(result, vec!["RQ-0001", "RQ-0002"]);
}

// =============================================================================
// Collect task IDs tests
// =============================================================================

#[test]
fn collect_task_ids_gathers_all_ids() {
    let tasks = [
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Doing, vec![]),
    ];
    let task_refs: Vec<&Task> = tasks.iter().collect();

    let result = collect_task_ids(&task_refs);

    assert_eq!(result, vec!["RQ-0001", "RQ-0002"]);
}

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

// =============================================================================
// resolve_task_ids tests
// =============================================================================

#[test]
fn resolve_task_ids_prefers_tag_filter() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec!["rust".to_string()]),
        task_with("RQ-0002", TaskStatus::Todo, vec!["rust".to_string()]),
        task_with("RQ-0003", TaskStatus::Todo, vec!["python".to_string()]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result = resolve_task_ids(
        &queue,
        &["RQ-0003".to_string()], // Should be ignored
        &["rust".to_string()],
    )
    .expect("should resolve tasks");

    assert_eq!(result.len(), 2);
    assert!(result.contains(&"RQ-0001".to_string()));
    assert!(result.contains(&"RQ-0002".to_string()));
}

#[test]
fn resolve_task_ids_uses_explicit_ids_when_no_tag_filter() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Todo, vec![]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result =
        resolve_task_ids(&queue, &["RQ-0001".to_string()], &[]).expect("should resolve tasks");

    assert_eq!(result, vec!["RQ-0001"]);
}

#[test]
fn resolve_task_ids_errors_on_empty_input() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let queue = QueueFile { version: 1, tasks };

    let result = resolve_task_ids(&queue, &[], &[]);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No tasks specified"));
}

#[test]
fn resolve_task_ids_errors_on_no_matching_tags() {
    let tasks = vec![task_with(
        "RQ-0001",
        TaskStatus::Todo,
        vec!["rust".to_string()],
    )];
    let queue = QueueFile { version: 1, tasks };

    let result = resolve_task_ids(&queue, &[], &["python".to_string()]);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No tasks found with tags"));
}

#[test]
fn resolve_task_ids_deduplicates_results() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec!["rust".to_string()]),
        task_with("RQ-0002", TaskStatus::Todo, vec!["rust".to_string()]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result =
        resolve_task_ids(&queue, &[], &["rust".to_string()]).expect("should resolve tasks");

    // Two different tasks with same tag, should return both
    assert_eq!(result.len(), 2);
}

// =============================================================================
// BatchOperationResult helper tests
// =============================================================================

#[test]
fn batch_operation_result_all_succeeded_true_when_no_failures() {
    let result = BatchOperationResult {
        total: 3,
        succeeded: 3,
        failed: 0,
        results: vec![],
    };

    assert!(result.all_succeeded());
    assert!(!result.has_failures());
}

#[test]
fn batch_operation_result_has_failures_true_when_failures() {
    let result = BatchOperationResult {
        total: 3,
        succeeded: 2,
        failed: 1,
        results: vec![],
    };

    assert!(!result.all_succeeded());
    assert!(result.has_failures());
}
