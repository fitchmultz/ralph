//! Tests for `edit.rs` operations (`apply_task_edit` and parsing/validation behavior).
//!
//! Responsibilities:
//! - Validate edit behavior and error context for task updates.
//! - Cover success paths and key failure modes for edits.
//!
//! Does not handle:
//! - Integration with on-disk queue persistence or CLI parsing.
//! - End-to-end TUI edit flows.
//!
//! Assumptions/invariants:
//! - Helper builders in this module construct valid baseline tasks.
//! - Errors are surfaced as user-facing strings.

use super::*;

#[test]
fn apply_task_edit_sets_status_from_input() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let now = "2026-01-19T00:00:00Z";
    let now_canon = canonical_rfc3339(now);
    apply_task_edit(
        &mut queue,
        None,
        "RQ-0001",
        TaskEditKey::Status,
        "done",
        now,
        "RQ",
        4,
    )?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.updated_at.as_deref(), Some(now_canon.as_str()));
    assert_eq!(t.completed_at.as_deref(), Some(now_canon.as_str()));

    Ok(())
}

#[test]
fn apply_task_edit_rejects_invalid_status_input() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let err = apply_task_edit(
        &mut queue,
        None,
        "RQ-0001",
        TaskEditKey::Status,
        "paused",
        "2026-01-19T00:00:00Z",
        "RQ",
        4,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Queue edit failed"));
    assert!(msg.contains("field=status"));
    assert_eq!(queue.tasks[0].status, TaskStatus::Todo);
}

#[test]
fn apply_task_edit_rejects_empty_title_with_context() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let err = apply_task_edit(
        &mut queue,
        None,
        "RQ-0001",
        TaskEditKey::Title,
        "   ",
        "2026-01-19T00:00:00Z",
        "RQ",
        4,
    )
    .unwrap_err();

    let msg = format!("{err}");
    assert!(msg.contains("Queue edit failed"));
    assert!(msg.contains("task_id=RQ-0001"));
    assert!(msg.contains("field=title"));
}

#[test]
fn apply_task_edit_rejects_custom_field_entries_without_equals() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let err = apply_task_edit(
        &mut queue,
        None,
        "RQ-0001",
        TaskEditKey::CustomFields,
        "severity",
        "2026-01-19T00:00:00Z",
        "RQ",
        4,
    )
    .unwrap_err();

    let msg = format!("{err}");
    assert!(msg.contains("Queue edit failed"));
    assert!(msg.contains("task_id=RQ-0001"));
    assert!(msg.contains("field=custom_fields"));
}
