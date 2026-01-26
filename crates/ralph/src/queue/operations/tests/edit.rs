//! Tests for `edit.rs` operations (`apply_task_edit` and parsing/validation behavior).

use super::*;

#[test]
fn apply_task_edit_sets_status_from_input() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let now = "2026-01-19T00:00:00Z";
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
    assert_eq!(t.updated_at.as_deref(), Some(now));
    assert_eq!(t.completed_at.as_deref(), Some(now));

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
    assert!(format!("{err}").contains("Invalid status"));
    assert_eq!(queue.tasks[0].status, TaskStatus::Todo);
}
