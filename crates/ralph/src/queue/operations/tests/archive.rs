//! Tests for `archive.rs` operations (archiving terminal tasks).

use super::*;

#[test]
fn archive_terminal_tasks_moves_done_and_rejected() -> anyhow::Result<()> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");

    let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "done",
                    "title": "Done task",
                    "priority": "medium",
                    "tags": [],
                    "scope": [],
                    "evidence": [],
                    "plan": [],
                    "notes": [],
                    "request": null,
                    "created_at": "2026-01-20T00:00:00Z",
                    "updated_at": "2026-01-20T00:00:00Z",
                    "completed_at": "2026-01-20T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                },
                {
                    "id": "RQ-0002",
                    "status": "rejected",
                    "title": "Rejected task",
                    "priority": "medium",
                    "tags": [],
                    "scope": [],
                    "evidence": [],
                    "plan": [],
                    "notes": [],
                    "request": null,
                    "created_at": "2026-01-20T00:00:00Z",
                    "updated_at": "2026-01-20T00:00:00Z",
                    "completed_at": "2026-01-20T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                },
                {
                    "id": "RQ-0003",
                    "status": "todo",
                    "title": "Todo task",
                    "priority": "medium",
                    "tags": [],
                    "scope": [],
                    "evidence": [],
                    "plan": [],
                    "notes": [],
                    "request": null,
                    "created_at": "2026-01-20T00:00:00Z",
                    "updated_at": "2026-01-20T00:00:00Z",
                    "completed_at": null,
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
    std::fs::write(&queue_path, queue_json)?;

    let report = archive_terminal_tasks(&queue_path, &done_path, "RQ", 4)?;

    // Check report
    assert_eq!(report.moved_ids.len(), 2);
    assert!(report.moved_ids.contains(&"RQ-0001".to_string()));
    assert!(report.moved_ids.contains(&"RQ-0002".to_string()));

    // Check queue file (should only have RQ-0003)
    let queue_content = std::fs::read_to_string(&queue_path)?;
    let queue: QueueFile = serde_json::from_str(&queue_content)?;
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0003");

    // Check done file (should have RQ-0001 and RQ-0002)
    let done_content = std::fs::read_to_string(&done_path)?;
    let done: QueueFile = serde_json::from_str(&done_content)?;
    assert_eq!(done.tasks.len(), 2);
    let ids: Vec<String> = done.tasks.iter().map(|t| t.id.clone()).collect();
    assert!(ids.contains(&"RQ-0001".to_string()));
    assert!(ids.contains(&"RQ-0002".to_string()));

    Ok(())
}

#[test]
fn archive_terminal_tasks_stamps_missing_completed_at() -> anyhow::Result<()> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");

    // Terminal task missing completed_at (this is bug scenario).
    let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "done",
                    "title": "Done task",
                    "priority": "medium",
                    "tags": [],
                    "scope": [],
                    "evidence": [],
                    "plan": [],
                    "notes": [],
                    "request": null,
                    "created_at": "2026-01-20T00:00:00Z",
                    "updated_at": "2026-01-20T00:00:00Z",
                    "completed_at": null,
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
    std::fs::write(&queue_path, queue_json)?;

    let report = archive_terminal_tasks(&queue_path, &done_path, "RQ", 4)?;
    assert_eq!(report.moved_ids, vec!["RQ-0001".to_string()]);

    let done_content = std::fs::read_to_string(&done_path)?;
    let done: QueueFile = serde_json::from_str(&done_content)?;
    assert_eq!(done.tasks.len(), 1);

    let completed_at = done.tasks[0]
        .completed_at
        .as_deref()
        .expect("completed_at should be stamped");

    // Ensure it is RFC3339 parseable.
    crate::timeutil::parse_rfc3339(completed_at).expect("completed_at must be RFC3339");

    Ok(())
}

#[test]
fn archive_terminal_tasks_backfills_existing_done_without_moves() -> anyhow::Result<()> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");

    let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0003",
                    "status": "todo",
                    "title": "Todo task",
                    "priority": "medium",
                    "tags": [],
                    "scope": [],
                    "evidence": [],
                    "plan": [],
                    "notes": [],
                    "request": null,
                    "created_at": "2026-01-20T00:00:00Z",
                    "updated_at": "2026-01-20T00:00:00Z",
                    "completed_at": null,
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
    std::fs::write(&queue_path, queue_json)?;

    let done_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "done",
                    "title": "Done task",
                    "priority": "medium",
                    "tags": [],
                    "scope": [],
                    "evidence": [],
                    "plan": [],
                    "notes": [],
                    "request": null,
                    "created_at": "2026-01-20T00:00:00Z",
                    "updated_at": "2026-01-20T00:00:00Z",
                    "completed_at": null,
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
    std::fs::write(&done_path, done_json)?;

    let report = archive_terminal_tasks(&queue_path, &done_path, "RQ", 4)?;
    assert!(report.moved_ids.is_empty());

    let done_content = std::fs::read_to_string(&done_path)?;
    let done: QueueFile = serde_json::from_str(&done_content)?;
    let completed_at = done.tasks[0]
        .completed_at
        .as_deref()
        .expect("completed_at should be backfilled");

    crate::timeutil::parse_rfc3339(completed_at).expect("completed_at must be RFC3339");

    Ok(())
}

#[test]
fn archive_terminal_tasks_in_memory_stamps_timestamps() -> anyhow::Result<()> {
    let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec![]);
    done_task.updated_at = None;
    done_task.completed_at = None;

    let mut rejected_task = task_with("RQ-0002", TaskStatus::Rejected, vec![]);
    rejected_task.updated_at = Some("2026-01-10T00:00:00Z".to_string());
    rejected_task.completed_at = Some("2026-01-10T00:00:00Z".to_string());

    let todo_task = task_with("RQ-0003", TaskStatus::Todo, vec![]);

    let mut active = QueueFile {
        version: 1,
        tasks: vec![done_task, rejected_task, todo_task],
    };
    let mut done = QueueFile::default();

    let now = "2026-01-22T00:00:00Z";
    let now_canon = canonical_rfc3339(now);
    let report = archive_terminal_tasks_in_memory(&mut active, &mut done, now)?;

    assert_eq!(report.moved_ids.len(), 2);
    assert!(report.moved_ids.contains(&"RQ-0001".to_string()));
    assert!(report.moved_ids.contains(&"RQ-0002".to_string()));
    assert_eq!(active.tasks.len(), 1);
    assert_eq!(active.tasks[0].id, "RQ-0003");
    assert_eq!(done.tasks.len(), 2);

    let done_archived = done
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("RQ-0001 archived");
    assert_eq!(
        done_archived.updated_at.as_deref(),
        Some(now_canon.as_str())
    );
    assert_eq!(
        done_archived.completed_at.as_deref(),
        Some(now_canon.as_str())
    );

    let rejected_archived = done
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("RQ-0002 archived");
    assert_eq!(
        rejected_archived.updated_at.as_deref(),
        Some(now_canon.as_str())
    );
    assert_eq!(
        rejected_archived.completed_at.as_deref(),
        Some("2026-01-10T00:00:00Z")
    );

    Ok(())
}

#[test]
fn archive_terminal_tasks_in_memory_rejects_invalid_rfc3339() {
    let mut active = QueueFile::default();
    let mut done = QueueFile::default();

    let err =
        archive_terminal_tasks_in_memory(&mut active, &mut done, "not-a-timestamp").unwrap_err();
    assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
}
