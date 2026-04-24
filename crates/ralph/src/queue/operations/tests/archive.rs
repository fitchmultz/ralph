//! Tests for `archive.rs` operations (archiving terminal tasks).
//!
//! Purpose:
//! - Tests for `archive.rs` operations (archiving terminal tasks).
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

    let report = archive_terminal_tasks(&queue_path, &done_path, "RQ", 4, 10)?;

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

    let report = archive_terminal_tasks(&queue_path, &done_path, "RQ", 4, 10)?;
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

    let report = archive_terminal_tasks(&queue_path, &done_path, "RQ", 4, 10)?;
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

#[test]
fn archive_terminal_tasks_older_than_days_zero_delegates_to_immediate() -> anyhow::Result<()> {
    use crate::contracts::TaskStatus;

    // Task completed 10 days ago
    let mut old_done = task_with("RQ-0001", TaskStatus::Done, vec![]);
    old_done.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    // Task completed 1 day ago
    let mut recent_done = task_with("RQ-0002", TaskStatus::Done, vec![]);
    recent_done.completed_at = Some("2026-01-10T00:00:00Z".to_string());

    // Todo task (not terminal)
    let todo_task = task_with("RQ-0003", TaskStatus::Todo, vec![]);

    let mut active = QueueFile {
        version: 1,
        tasks: vec![old_done, recent_done, todo_task],
    };
    let mut done = QueueFile::default();

    let now = "2026-01-11T00:00:00Z";
    let report = archive_terminal_tasks_older_than_days_in_memory(&mut active, &mut done, now, 0)?;

    // With after_days=0, all terminal tasks should be archived regardless of age
    assert_eq!(report.moved_ids.len(), 2);
    assert!(report.moved_ids.contains(&"RQ-0001".to_string()));
    assert!(report.moved_ids.contains(&"RQ-0002".to_string()));
    assert_eq!(active.tasks.len(), 1);
    assert_eq!(active.tasks[0].id, "RQ-0003");

    Ok(())
}

#[test]
fn archive_terminal_tasks_older_than_days_respects_age_cutoff() -> anyhow::Result<()> {
    use crate::contracts::TaskStatus;

    // Task completed 10 days ago (older than 7 day cutoff)
    let mut old_done = task_with("RQ-0001", TaskStatus::Done, vec![]);
    old_done.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    // Task completed 1 day ago (newer than 7 day cutoff)
    let mut recent_done = task_with("RQ-0002", TaskStatus::Done, vec![]);
    recent_done.completed_at = Some("2026-01-10T00:00:00Z".to_string());

    // Task completed exactly 7 days ago (at the cutoff, should be archived)
    let mut exact_cutoff = task_with("RQ-0003", TaskStatus::Done, vec![]);
    exact_cutoff.completed_at = Some("2026-01-04T00:00:00Z".to_string());

    // Todo task (not terminal)
    let todo_task = task_with("RQ-0004", TaskStatus::Todo, vec![]);

    let mut active = QueueFile {
        version: 1,
        tasks: vec![old_done, recent_done, exact_cutoff, todo_task],
    };
    let mut done = QueueFile::default();

    let now = "2026-01-11T00:00:00Z";
    let report = archive_terminal_tasks_older_than_days_in_memory(&mut active, &mut done, now, 7)?;

    // Only tasks >= 7 days old should be archived
    assert_eq!(report.moved_ids.len(), 2);
    assert!(report.moved_ids.contains(&"RQ-0001".to_string()));
    assert!(report.moved_ids.contains(&"RQ-0003".to_string()));
    assert!(!report.moved_ids.contains(&"RQ-0002".to_string())); // Too recent

    // Recent done task should remain in active
    assert_eq!(active.tasks.len(), 2);
    let remaining_ids: Vec<String> = active.tasks.iter().map(|t| t.id.clone()).collect();
    assert!(remaining_ids.contains(&"RQ-0002".to_string()));
    assert!(remaining_ids.contains(&"RQ-0004".to_string()));

    Ok(())
}

#[test]
fn archive_terminal_tasks_older_than_days_skips_missing_completed_at() -> anyhow::Result<()> {
    use crate::contracts::TaskStatus;

    // Done task with missing completed_at
    let mut done_no_timestamp = task_with("RQ-0001", TaskStatus::Done, vec![]);
    done_no_timestamp.completed_at = None;

    // Rejected task with empty completed_at
    let mut rejected_empty_timestamp = task_with("RQ-0002", TaskStatus::Rejected, vec![]);
    rejected_empty_timestamp.completed_at = Some("".to_string());

    // Done task with valid old completed_at (should be archived)
    let mut old_done = task_with("RQ-0003", TaskStatus::Done, vec![]);
    old_done.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    let mut active = QueueFile {
        version: 1,
        tasks: vec![done_no_timestamp, rejected_empty_timestamp, old_done],
    };
    let mut done = QueueFile::default();

    let now = "2026-01-11T00:00:00Z";
    let report = archive_terminal_tasks_older_than_days_in_memory(&mut active, &mut done, now, 7)?;

    // Only the task with a valid timestamp should be archived
    assert_eq!(report.moved_ids.len(), 1);
    assert!(report.moved_ids.contains(&"RQ-0003".to_string()));

    // Tasks with missing/empty timestamps should remain in active
    assert_eq!(active.tasks.len(), 2);
    let remaining_ids: Vec<String> = active.tasks.iter().map(|t| t.id.clone()).collect();
    assert!(remaining_ids.contains(&"RQ-0001".to_string()));
    assert!(remaining_ids.contains(&"RQ-0002".to_string()));

    Ok(())
}

#[test]
fn archive_terminal_tasks_older_than_days_skips_invalid_completed_at() -> anyhow::Result<()> {
    use crate::contracts::TaskStatus;

    // Done task with invalid completed_at
    let mut done_invalid = task_with("RQ-0001", TaskStatus::Done, vec![]);
    done_invalid.completed_at = Some("not-a-timestamp".to_string());

    // Done task with valid old completed_at (should be archived)
    let mut old_done = task_with("RQ-0002", TaskStatus::Done, vec![]);
    old_done.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    let mut active = QueueFile {
        version: 1,
        tasks: vec![done_invalid, old_done],
    };
    let mut done = QueueFile::default();

    let now = "2026-01-11T00:00:00Z";
    let report = archive_terminal_tasks_older_than_days_in_memory(&mut active, &mut done, now, 7)?;

    // Only the task with a valid timestamp should be archived
    assert_eq!(report.moved_ids.len(), 1);
    assert!(report.moved_ids.contains(&"RQ-0002".to_string()));

    // Task with invalid timestamp should remain in active
    assert_eq!(active.tasks.len(), 1);
    assert_eq!(active.tasks[0].id, "RQ-0001");

    Ok(())
}

#[test]
fn maybe_archive_terminal_tasks_in_memory_disabled_when_none() -> anyhow::Result<()> {
    use crate::contracts::TaskStatus;

    let mut old_done = task_with("RQ-0001", TaskStatus::Done, vec![]);
    old_done.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    let mut active = QueueFile {
        version: 1,
        tasks: vec![old_done],
    };
    let mut done = QueueFile::default();

    let now = "2026-01-11T00:00:00Z";
    let report = maybe_archive_terminal_tasks_in_memory(&mut active, &mut done, now, None)?;

    // When disabled (None), no tasks should be archived
    assert!(report.moved_ids.is_empty());
    assert_eq!(active.tasks.len(), 1);
    assert!(done.tasks.is_empty());

    Ok(())
}

#[test]
fn archive_report_contains_specific_task_ids() -> anyhow::Result<()> {
    use crate::contracts::TaskStatus;

    // Create multiple terminal tasks with different IDs
    let mut task1 = task_with("RQ-0001", TaskStatus::Done, vec![]);
    task1.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    let mut task2 = task_with("RQ-0002", TaskStatus::Rejected, vec![]);
    task2.completed_at = Some("2026-01-02T00:00:00Z".to_string());

    let mut task3 = task_with("RQ-0003", TaskStatus::Done, vec![]);
    task3.completed_at = Some("2026-01-03T00:00:00Z".to_string());

    let mut active = QueueFile {
        version: 1,
        tasks: vec![task1, task2, task3],
    };
    let mut done = QueueFile::default();

    let now = "2026-01-11T00:00:00Z";
    let report = archive_terminal_tasks_in_memory(&mut active, &mut done, now)?;

    // Report should contain all archived task IDs
    assert_eq!(report.moved_ids.len(), 3);
    assert!(report.moved_ids.contains(&"RQ-0001".to_string()));
    assert!(report.moved_ids.contains(&"RQ-0002".to_string()));
    assert!(report.moved_ids.contains(&"RQ-0003".to_string()));

    // All tasks should be archived
    assert!(active.tasks.is_empty());
    assert_eq!(done.tasks.len(), 3);

    Ok(())
}
