//! Tests for structured task mutation transactions.
//!
//! Responsibilities:
//! - Validate atomic multi-field edits across one or more tasks.
//! - Verify optimistic-lock conflicts leave the queue unchanged.
//! - Ensure status-to-doing mutations preserve started_at semantics through the transaction path.

use super::*;
use crate::queue::operations::{
    TaskFieldEdit, TaskMutationRequest, TaskMutationSpec, apply_task_mutation_request,
};

#[test]
fn task_mutation_request_applies_multiple_fields_atomically() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let request = TaskMutationRequest {
        version: 1,
        atomic: true,
        tasks: vec![TaskMutationSpec {
            task_id: "RQ-0001".to_string(),
            expected_updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            edits: vec![
                TaskFieldEdit {
                    field: "title".to_string(),
                    value: "Updated task".to_string(),
                },
                TaskFieldEdit {
                    field: "priority".to_string(),
                    value: "high".to_string(),
                },
                TaskFieldEdit {
                    field: "description".to_string(),
                    value: "Updated description".to_string(),
                },
            ],
        }],
    };

    let report = apply_task_mutation_request(
        &mut queue,
        None,
        &request,
        "2026-01-21T12:00:00Z",
        "RQ",
        4,
        10,
    )
    .expect("mutation should succeed");

    assert_eq!(report.tasks.len(), 1);
    let task = &queue.tasks[0];
    assert_eq!(task.title, "Updated task");
    assert_eq!(task.priority, TaskPriority::High);
    assert_eq!(task.description.as_deref(), Some("Updated description"));
}

#[test]
fn task_mutation_request_conflict_keeps_queue_unchanged() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let original = queue.tasks[0].clone();
    let request = TaskMutationRequest {
        version: 1,
        atomic: true,
        tasks: vec![TaskMutationSpec {
            task_id: "RQ-0001".to_string(),
            expected_updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            edits: vec![TaskFieldEdit {
                field: "title".to_string(),
                value: "Should not apply".to_string(),
            }],
        }],
    };

    let err = apply_task_mutation_request(
        &mut queue,
        None,
        &request,
        "2026-01-21T12:00:00Z",
        "RQ",
        4,
        10,
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("Task mutation conflict"));
    assert_eq!(queue.tasks[0].title, original.title);
    assert_eq!(queue.tasks[0].updated_at, original.updated_at);
}

#[test]
fn task_mutation_request_status_doing_sets_started_at_once() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let request = TaskMutationRequest {
        version: 1,
        atomic: true,
        tasks: vec![TaskMutationSpec {
            task_id: "RQ-0001".to_string(),
            expected_updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            edits: vec![TaskFieldEdit {
                field: "status".to_string(),
                value: "doing".to_string(),
            }],
        }],
    };

    apply_task_mutation_request(
        &mut queue,
        None,
        &request,
        "2026-01-21T12:00:00Z",
        "RQ",
        4,
        10,
    )
    .expect("status mutation should succeed");

    let task = &queue.tasks[0];
    assert_eq!(task.status, TaskStatus::Doing);
    let started_at = task
        .started_at
        .as_deref()
        .expect("doing transition should set started_at");
    let started_at =
        crate::timeutil::parse_rfc3339(started_at).expect("started_at should remain valid RFC3339");
    let expected = crate::timeutil::parse_rfc3339("2026-01-21T12:00:00Z")
        .expect("expected timestamp should parse");
    assert_eq!(started_at, expected);
}
