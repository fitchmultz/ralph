//! Tests for `fields.rs` operations.
//!
//! Responsibilities:
//! - Validate custom field mutation behavior and error context.
//! - Ensure timestamps and custom field values update as expected.
//!
//! Does not handle:
//! - Queue persistence, CLI parsing, or multi-field edits.
//! - Validation of non-custom task fields.
//!
//! Assumptions/invariants:
//! - Task helper functions return minimal valid tasks.
//! - Custom field keys are interpreted literally without normalization.

use super::*;

#[test]
fn set_field_rejects_missing_task_id() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let err = set_field(
        &mut queue,
        "   ",
        "severity",
        "high",
        "2026-01-19T00:00:00Z",
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Queue custom field set failed"));
    assert!(msg.contains("missing task_id"));
}

#[test]
fn set_field_rejects_whitespace_key() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let err = set_field(
        &mut queue,
        "RQ-0001",
        "bad key",
        "high",
        "2026-01-19T00:00:00Z",
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("task_id=RQ-0001"));
    assert!(msg.contains("contains whitespace"));
}

#[test]
fn set_field_updates_custom_field_value() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    set_field(
        &mut queue,
        "RQ-0001",
        "severity",
        "high",
        "2026-01-19T00:00:00Z",
    )?;

    let task = &queue.tasks[0];
    assert_eq!(
        task.custom_fields.get("severity").map(String::as_str),
        Some("high")
    );
    let now_canon = canonical_rfc3339("2026-01-19T00:00:00Z");
    assert_eq!(task.updated_at.as_deref(), Some(now_canon.as_str()));
    Ok(())
}
