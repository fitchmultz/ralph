//! Tests for `status.rs` operations (status transitions + completion workflow).
//!
//! Purpose:
//! - Tests for `status.rs` operations (status transitions + completion workflow).
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;

#[test]
fn set_status_rejects_invalid_rfc3339() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let err = set_status(&mut queue, "RQ-0001", TaskStatus::Doing, "invalid", None).unwrap_err();
    assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
    Ok(())
}

#[test]
fn set_status_updates_timestamps_and_fields() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let now = "2026-01-17T00:00:00Z";
    let now_canon = canonical_rfc3339(now);
    set_status(
        &mut queue,
        "RQ-0001",
        TaskStatus::Doing,
        now,
        Some("started"),
    )?;
    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Doing);
    assert_eq!(t.updated_at.as_deref(), Some(now_canon.as_str()));
    assert_eq!(t.completed_at, None);
    assert_eq!(t.notes, vec!["started".to_string()]);

    let now2 = "2026-01-17T00:02:00Z";
    let now2_canon = canonical_rfc3339(now2);
    set_status(
        &mut queue,
        "RQ-0001",
        TaskStatus::Done,
        now2,
        Some("completed"),
    )?;
    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.updated_at.as_deref(), Some(now2_canon.as_str()));
    assert_eq!(t.completed_at.as_deref(), Some(now2_canon.as_str()));
    assert!(t.notes.iter().any(|n| n == "completed"));

    Ok(())
}

#[test]
fn set_status_preserves_existing_completed_at_on_terminal_transition() -> anyhow::Result<()> {
    let mut t = task_with("RQ-0001", TaskStatus::Doing, vec!["code".to_string()]);
    t.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    let mut queue = QueueFile {
        version: 1,
        tasks: vec![t],
    };

    let now = "2026-01-17T00:02:00Z";
    let now_canon = canonical_rfc3339(now);
    set_status(&mut queue, "RQ-0001", TaskStatus::Done, now, None)?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.updated_at.as_deref(), Some(now_canon.as_str()));
    assert_eq!(t.completed_at.as_deref(), Some("2026-01-01T00:00:00Z"));

    Ok(())
}

#[test]
fn set_status_backfills_empty_completed_at_on_terminal_transition() -> anyhow::Result<()> {
    let mut t = task_with("RQ-0001", TaskStatus::Doing, vec!["code".to_string()]);
    t.completed_at = Some("   ".to_string());

    let mut queue = QueueFile {
        version: 1,
        tasks: vec![t],
    };

    let now = "2026-01-17T00:02:00Z";
    let now_canon = canonical_rfc3339(now);
    set_status(&mut queue, "RQ-0001", TaskStatus::Done, now, None)?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.completed_at.as_deref(), Some(now_canon.as_str()));

    Ok(())
}

#[test]
fn set_status_clears_completed_at_on_non_terminal_transition() -> anyhow::Result<()> {
    let mut t = task_with("RQ-0001", TaskStatus::Done, vec!["code".to_string()]);
    t.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    let mut queue = QueueFile {
        version: 1,
        tasks: vec![t],
    };

    let now = "2026-01-17T00:02:00Z";
    let now_canon = canonical_rfc3339(now);
    set_status(&mut queue, "RQ-0001", TaskStatus::Todo, now, None)?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Todo);
    assert_eq!(t.updated_at.as_deref(), Some(now_canon.as_str()));
    assert_eq!(t.completed_at, None);

    Ok(())
}

#[test]
fn set_status_redacts_note() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let now = "2026-01-17T00:00:00Z";
    set_status(
        &mut queue,
        "RQ-0001",
        TaskStatus::Doing,
        now,
        Some("API_KEY=abc12345"),
    )?;

    let t = &queue.tasks[0];
    assert_eq!(t.notes, vec!["API_KEY=[REDACTED]".to_string()]);

    Ok(())
}

#[test]
fn set_status_sanitizes_leading_backticks() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let now = "2026-01-17T00:00:00Z";
    set_status(
        &mut queue,
        "RQ-0001",
        TaskStatus::Doing,
        now,
        Some("`make ci` failed"),
    )?;

    let t = &queue.tasks[0];
    assert_eq!(t.notes, vec!["`make ci` failed".to_string()]);

    Ok(())
}

#[test]
fn promote_draft_to_todo_updates_status_and_timestamps() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Draft,
            vec!["code".to_string()],
        )],
    };

    let now = "2026-01-17T00:00:00Z";
    let now_canon = canonical_rfc3339(now);
    promote_draft_to_todo(&mut queue, "RQ-0001", now, Some("ready"))?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Todo);
    assert_eq!(t.updated_at.as_deref(), Some(now_canon.as_str()));
    assert_eq!(t.completed_at, None);
    assert!(t.notes.iter().any(|n| n == "ready"));
    Ok(())
}

#[test]
fn promote_draft_to_todo_rejects_non_draft() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["code".to_string()],
        )],
    };

    let err =
        promote_draft_to_todo(&mut queue, "RQ-0001", "2026-01-17T00:00:00Z", None).unwrap_err();
    assert!(format!("{err}").contains("not in draft status"));
}

#[test]
fn complete_task_moves_task_from_queue_to_done() -> anyhow::Result<()> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");

    let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "doing",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
    std::fs::write(&queue_path, queue_json)?;

    let now = "2026-01-20T12:00:00Z";
    let now_canon = canonical_rfc3339(now);
    complete_task(
        &queue_path,
        &done_path,
        "RQ-0001",
        TaskStatus::Done,
        now,
        &["Test note".to_string()],
        "RQ",
        4,
        10,
        None,
    )?;

    let queue_content = std::fs::read_to_string(&queue_path)?;
    let queue: QueueFile = serde_json::from_str(&queue_content)?;
    assert_eq!(queue.tasks.len(), 0);

    let done_content = std::fs::read_to_string(&done_path)?;
    let done: QueueFile = serde_json::from_str(&done_content)?;
    assert_eq!(done.tasks.len(), 1);
    assert_eq!(done.tasks[0].id, "RQ-0001");
    assert_eq!(done.tasks[0].status, TaskStatus::Done);
    assert_eq!(
        done.tasks[0].completed_at.as_deref(),
        Some(now_canon.as_str())
    );
    assert_eq!(
        done.tasks[0].updated_at.as_deref(),
        Some(now_canon.as_str())
    );
    assert_eq!(done.tasks[0].notes, vec!["Test note"]);

    Ok(())
}

#[test]
fn complete_task_allows_dependents_in_active_queue() -> anyhow::Result<()> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");

    let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "doing",
                    "title": "Dependency task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                },
                {
                    "id": "RQ-0002",
                    "status": "todo",
                    "title": "Dependent task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": ["RQ-0001"],
                    "custom_fields": {}
                }
            ]
        }"#;
    std::fs::write(&queue_path, queue_json)?;

    let now = "2026-01-20T12:00:00Z";
    complete_task(
        &queue_path,
        &done_path,
        "RQ-0001",
        TaskStatus::Done,
        now,
        &[],
        "RQ",
        4,
        10,
        None,
    )?;

    let queue_content = std::fs::read_to_string(&queue_path)?;
    let queue: QueueFile = serde_json::from_str(&queue_content)?;
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0002");

    let done_content = std::fs::read_to_string(&done_path)?;
    let done: QueueFile = serde_json::from_str(&done_content)?;
    assert_eq!(done.tasks.len(), 1);
    assert_eq!(done.tasks[0].id, "RQ-0001");

    Ok(())
}

#[test]
fn complete_task_rejects_non_terminal_status() -> anyhow::Result<()> {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut queue_file = NamedTempFile::new()?;
    let done_file = NamedTempFile::new()?;

    let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "doing",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
    queue_file.write_all(queue_json.as_bytes())?;
    queue_file.flush()?;

    let now = "2026-01-20T12:00:00Z";
    let err = complete_task(
        queue_file.path(),
        done_file.path(),
        "RQ-0001",
        TaskStatus::Todo,
        now,
        &[],
        "RQ",
        4,
        10,
        None,
    )
    .unwrap_err();
    assert!(
        format!("{err}")
            .to_lowercase()
            .contains("invalid completion status")
    );

    Ok(())
}

#[test]
fn complete_task_rejects_task_already_terminal() -> anyhow::Result<()> {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut queue_file = NamedTempFile::new()?;
    let done_file = NamedTempFile::new()?;

    let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "done",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "completed_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
    queue_file.write_all(queue_json.as_bytes())?;
    queue_file.flush()?;

    let now = "2026-01-20T12:00:00Z";
    let err = complete_task(
        queue_file.path(),
        done_file.path(),
        "RQ-0001",
        TaskStatus::Done,
        now,
        &[],
        "RQ",
        4,
        10,
        None,
    )
    .unwrap_err();
    assert!(
        format!("{err}")
            .to_lowercase()
            .contains("already in a terminal state")
    );

    Ok(())
}

#[test]
fn complete_task_rejects_nonexistent_task() -> anyhow::Result<()> {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut queue_file = NamedTempFile::new()?;
    let done_file = NamedTempFile::new()?;

    let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0002",
                    "status": "todo",
                    "title": "Other task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
    queue_file.write_all(queue_json.as_bytes())?;
    queue_file.flush()?;

    let now = "2026-01-20T12:00:00Z";
    let err = complete_task(
        queue_file.path(),
        done_file.path(),
        "RQ-0001",
        TaskStatus::Done,
        now,
        &[],
        "RQ",
        4,
        10,
        None,
    )
    .unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("task not found"));

    Ok(())
}
