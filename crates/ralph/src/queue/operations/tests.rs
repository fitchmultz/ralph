//! Unit tests for queue task operations.

use super::*;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use std::collections::HashMap;

fn task(id: &str) -> Task {
    task_with(id, TaskStatus::Todo, vec!["code".to_string()])
}

fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags,
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        depends_on: vec![],
        custom_fields: HashMap::new(),
    }
}

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
    set_status(
        &mut queue,
        "RQ-0001",
        TaskStatus::Doing,
        now,
        Some("started"),
    )?;
    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Doing);
    assert_eq!(t.updated_at.as_deref(), Some(now));
    assert_eq!(t.completed_at, None);
    assert_eq!(t.notes, vec!["started".to_string()]);

    let now2 = "2026-01-17T00:02:00Z";
    set_status(
        &mut queue,
        "RQ-0001",
        TaskStatus::Done,
        now2,
        Some("completed"),
    )?;
    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.updated_at.as_deref(), Some(now2));
    assert_eq!(t.completed_at.as_deref(), Some(now2));
    assert!(t.notes.iter().any(|n| n == "completed"));

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
    promote_draft_to_todo(&mut queue, "RQ-0001", now, Some("ready"))?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Todo);
    assert_eq!(t.updated_at.as_deref(), Some(now));
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
fn added_tasks_returns_titles_for_new_tasks() {
    let before = task_id_set(&QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    });
    let after = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0002")],
    };
    let added = added_tasks(&before, &after);
    assert_eq!(
        added,
        vec![("RQ-0002".to_string(), "Test task".to_string())]
    );
}

#[test]
fn backfill_missing_fields_applies_defaults() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0002".to_string(),
            status: TaskStatus::Todo,
            title: "Title".to_string(),
            priority: Default::default(),
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            depends_on: vec![],
            custom_fields: HashMap::new(),
        }],
    };
    backfill_missing_fields(
        &mut queue,
        &["RQ-0002".to_string()],
        "req",
        "2026-01-18T00:00:00Z",
    );
    let task = &queue.tasks[0];
    assert_eq!(task.request.as_deref(), Some("req"));
    assert_eq!(task.created_at.as_deref(), Some("2026-01-18T00:00:00Z"));
    assert_eq!(task.updated_at.as_deref(), Some("2026-01-18T00:00:00Z"));
}

#[test]
fn backfill_missing_fields_populates_request() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    queue.tasks[0].request = None;

    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string()],
        "default request",
        "2026-01-18T00:00:00Z",
    );

    assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
}

#[test]
fn backfill_missing_fields_populates_timestamps() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    queue.tasks[0].created_at = None;
    queue.tasks[0].updated_at = None;

    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string()],
        "default request",
        "2026-01-18T12:34:56Z",
    );

    assert_eq!(
        queue.tasks[0].created_at,
        Some("2026-01-18T12:34:56Z".to_string())
    );
    assert_eq!(
        queue.tasks[0].updated_at,
        Some("2026-01-18T12:34:56Z".to_string())
    );
}

#[test]
fn backfill_missing_fields_skips_existing_values() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string()],
        "new request",
        "2026-01-18T12:34:56Z",
    );

    assert_eq!(queue.tasks[0].request, Some("test request".to_string()));
    assert_eq!(
        queue.tasks[0].created_at,
        Some("2026-01-18T00:00:00Z".to_string())
    );
    assert_eq!(
        queue.tasks[0].updated_at,
        Some("2026-01-18T00:00:00Z".to_string())
    );
}

#[test]
fn backfill_missing_fields_only_affects_specified_ids() {
    let mut t1 = task("RQ-0001");
    t1.request = None;
    let t2 = task("RQ-0002");
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![t1, t2],
    };

    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string()],
        "backfilled request",
        "2026-01-18T12:34:56Z",
    );

    assert_eq!(
        queue.tasks[0].request,
        Some("backfilled request".to_string())
    );
    assert_eq!(queue.tasks[1].request, Some("test request".to_string()));
}

#[test]
fn backfill_missing_fields_handles_empty_string_as_missing() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    queue.tasks[0].request = Some("".to_string());
    queue.tasks[0].created_at = Some("".to_string());
    queue.tasks[0].updated_at = Some("".to_string());

    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string()],
        "default request",
        "2026-01-18T12:34:56Z",
    );

    assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
    assert_eq!(
        queue.tasks[0].created_at,
        Some("2026-01-18T12:34:56Z".to_string())
    );
    assert_eq!(
        queue.tasks[0].updated_at,
        Some("2026-01-18T12:34:56Z".to_string())
    );
}

#[test]
fn backfill_missing_fields_empty_now_skips() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    queue.tasks[0].created_at = None;
    queue.tasks[0].updated_at = None;

    backfill_missing_fields(&mut queue, &["RQ-0001".to_string()], "default request", "");

    assert_eq!(queue.tasks[0].created_at, None);
    assert_eq!(queue.tasks[0].updated_at, None);
}

#[test]
fn sort_tasks_by_priority_descending_orders_high_first() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![
            Task {
                id: "RQ-0002".to_string(),
                status: TaskStatus::Todo,
                title: "Low".to_string(),
                priority: TaskPriority::Low,
                tags: vec![],
                scope: vec![],
                evidence: vec![],
                plan: vec![],
                notes: vec![],
                request: None,
                agent: None,
                created_at: None,
                updated_at: None,
                completed_at: None,
                depends_on: vec![],
                custom_fields: HashMap::new(),
            },
            Task {
                id: "RQ-0001".to_string(),
                status: TaskStatus::Todo,
                title: "High".to_string(),
                priority: TaskPriority::High,
                tags: vec![],
                scope: vec![],
                evidence: vec![],
                plan: vec![],
                notes: vec![],
                request: None,
                agent: None,
                created_at: None,
                updated_at: None,
                completed_at: None,
                depends_on: vec![],
                custom_fields: HashMap::new(),
            },
        ],
    };

    sort_tasks_by_priority(&mut queue, true);

    assert_eq!(queue.tasks[0].priority, TaskPriority::High);
    assert_eq!(queue.tasks[1].priority, TaskPriority::Low);
}

#[test]
fn sort_tasks_by_priority_ascending() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![
            task_with("RQ-0001", TaskStatus::Todo, vec![]),
            task_with("RQ-0002", TaskStatus::Todo, vec![]),
            task_with("RQ-0003", TaskStatus::Todo, vec![]),
        ],
    };
    queue.tasks[0].priority = TaskPriority::Low;
    queue.tasks[1].priority = TaskPriority::Critical;
    queue.tasks[2].priority = TaskPriority::High;

    sort_tasks_by_priority(&mut queue, false);

    assert_eq!(queue.tasks[0].id, "RQ-0001");
    assert_eq!(queue.tasks[1].id, "RQ-0003");
    assert_eq!(queue.tasks[2].id, "RQ-0002");
}

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
fn delete_task_removes_task() -> anyhow::Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0002")],
    };

    let deleted = delete_task(&mut queue, "RQ-0002")?;
    assert!(deleted);
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0001");
    Ok(())
}

#[test]
fn task_id_set_ignores_empty_ids() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    queue.tasks.push(Task {
        id: "".to_string(),
        status: TaskStatus::Todo,
        title: "Bad".to_string(),
        priority: Default::default(),
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        depends_on: vec![],
        custom_fields: HashMap::new(),
    });

    let ids = task_id_set(&queue);
    assert_eq!(ids.len(), 1);
    assert!(ids.contains("RQ-0001"));
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
    complete_task(
        &queue_path,
        &done_path,
        "RQ-0001",
        TaskStatus::Done,
        now,
        &["Test note".to_string()],
        "RQ",
        4,
    )?;

    let queue_content = std::fs::read_to_string(&queue_path)?;
    let queue: QueueFile = serde_json::from_str(&queue_content)?;
    assert_eq!(queue.tasks.len(), 0);

    let done_content = std::fs::read_to_string(&done_path)?;
    let done: QueueFile = serde_json::from_str(&done_content)?;
    assert_eq!(done.tasks.len(), 1);
    assert_eq!(done.tasks[0].id, "RQ-0001");
    assert_eq!(done.tasks[0].status, TaskStatus::Done);
    assert_eq!(done.tasks[0].completed_at.as_deref(), Some(now));
    assert_eq!(done.tasks[0].updated_at.as_deref(), Some(now));
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
    )
    .unwrap_err();
    assert!(format!("{err}")
        .to_lowercase()
        .contains("invalid completion status"));

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
    )
    .unwrap_err();
    assert!(format!("{err}")
        .to_lowercase()
        .contains("already in a terminal state"));

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
    )
    .unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("task not found"));

    Ok(())
}
