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
fn set_status_preserves_existing_completed_at_on_terminal_transition() -> anyhow::Result<()> {
    let mut t = task_with("RQ-0001", TaskStatus::Doing, vec!["code".to_string()]);
    t.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    let mut queue = QueueFile {
        version: 1,
        tasks: vec![t],
    };

    let now = "2026-01-17T00:02:00Z";
    set_status(&mut queue, "RQ-0001", TaskStatus::Done, now, None)?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.updated_at.as_deref(), Some(now));
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
    set_status(&mut queue, "RQ-0001", TaskStatus::Done, now, None)?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.completed_at.as_deref(), Some(now));

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
    set_status(&mut queue, "RQ-0001", TaskStatus::Todo, now, None)?;

    let t = &queue.tasks[0];
    assert_eq!(t.status, TaskStatus::Todo);
    assert_eq!(t.updated_at.as_deref(), Some(now));
    assert_eq!(t.completed_at, None);

    Ok(())
}

#[test]
fn backfill_terminal_completed_at_updates_only_missing() -> anyhow::Result<()> {
    let mut done = task_with("RQ-0001", TaskStatus::Done, vec!["code".to_string()]);
    done.completed_at = None;

    let mut rejected = task_with("RQ-0002", TaskStatus::Rejected, vec!["code".to_string()]);
    rejected.completed_at = Some("   ".to_string());

    let mut todo = task_with("RQ-0003", TaskStatus::Todo, vec!["code".to_string()]);
    todo.completed_at = Some("2026-01-01T00:00:00Z".to_string());

    let mut queue = QueueFile {
        version: 1,
        tasks: vec![done, rejected, todo],
    };

    let now = "2026-01-17T00:00:00Z";
    let updated = backfill_terminal_completed_at(&mut queue, now);
    assert_eq!(updated, 2);

    assert_eq!(queue.tasks[0].completed_at.as_deref(), Some(now));
    assert_eq!(queue.tasks[1].completed_at.as_deref(), Some(now));
    assert_eq!(
        queue.tasks[2].completed_at.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );

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

#[test]
fn test_next_runnable_task_skips_blocked() {
    let mut blocked = task("RQ-0002");
    blocked.status = TaskStatus::Todo;
    blocked.depends_on = vec!["RQ-0003".to_string()]; // Depends on RQ-0003

    let mut blocker = task("RQ-0003");
    blocker.status = TaskStatus::Todo;

    let queue = QueueFile {
        version: 1,
        tasks: vec![blocked, blocker.clone()],
    };

    // blocked (RQ-0002) is first but blocked. blocker (RQ-0003) is second and runnable.
    // So next_runnable_task should return RQ-0003.
    let next = next_runnable_task(&queue, None).expect("should find runnable task");
    assert_eq!(next.id, "RQ-0003");
}

#[test]
fn select_runnable_task_index_prefers_doing() {
    let mut todo = task("RQ-0001");
    todo.status = TaskStatus::Todo;

    let mut doing = task("RQ-0002");
    doing.status = TaskStatus::Doing;

    let queue = QueueFile {
        version: 1,
        tasks: vec![todo, doing],
    };

    let idx = select_runnable_task_index(&queue, None, RunnableSelectionOptions::new(false, true))
        .expect("should select doing");
    assert_eq!(idx, 1);
}

#[test]
fn select_runnable_task_index_prefers_todo_over_draft() {
    let mut draft = task("RQ-0001");
    draft.status = TaskStatus::Draft;

    let mut todo = task("RQ-0002");
    todo.status = TaskStatus::Todo;

    let queue = QueueFile {
        version: 1,
        tasks: vec![draft, todo],
    };

    let idx = select_runnable_task_index(&queue, None, RunnableSelectionOptions::new(true, true))
        .expect("should select todo");
    assert_eq!(idx, 1);
}

#[test]
fn select_runnable_task_index_with_target_rejects_empty_id() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };

    let err = select_runnable_task_index_with_target(
        &queue,
        None,
        "   ",
        RunnableSelectionOptions::new(false, true),
    )
    .unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("empty"));
}

#[test]
fn select_runnable_task_index_with_target_rejects_draft_without_flag() {
    let mut draft = task("RQ-0001");
    draft.status = TaskStatus::Draft;

    let queue = QueueFile {
        version: 1,
        tasks: vec![draft],
    };

    let err = select_runnable_task_index_with_target(
        &queue,
        None,
        "RQ-0001",
        RunnableSelectionOptions::new(false, true),
    )
    .unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("include-draft"));
}

#[test]
fn select_runnable_task_index_with_target_rejects_unmet_dependencies() {
    let mut blocked = task("RQ-0001");
    blocked.status = TaskStatus::Todo;
    blocked.depends_on = vec!["RQ-0002".to_string()];

    let queue = QueueFile {
        version: 1,
        tasks: vec![blocked],
    };

    let err = select_runnable_task_index_with_target(
        &queue,
        None,
        "RQ-0001",
        RunnableSelectionOptions::new(false, true),
    )
    .unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("dependencies"));
}

#[test]
fn test_next_runnable_task_returns_unblocked() {
    let mut t1 = task("RQ-0002");
    t1.status = TaskStatus::Todo;
    t1.depends_on = vec!["RQ-0001".to_string()];

    // Dependency is done in active queue (or done queue)
    let mut t_dep = task("RQ-0001");
    t_dep.status = TaskStatus::Done;
    t_dep.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let queue = QueueFile {
        version: 1,
        tasks: vec![t1],
    };
    let done_queue = QueueFile {
        version: 1,
        tasks: vec![t_dep],
    };

    let next = next_runnable_task(&queue, Some(&done_queue)).expect("should find runnable task");
    assert_eq!(next.id, "RQ-0002");
}

#[test]
fn test_next_runnable_task_skips_missing_dep() {
    let mut t1 = task("RQ-0002");
    t1.status = TaskStatus::Todo;
    t1.depends_on = vec!["RQ-9999".to_string()]; // Missing

    let queue = QueueFile {
        version: 1,
        tasks: vec![t1],
    };

    let next = next_runnable_task(&queue, None);
    assert!(next.is_none());
}

#[test]
fn test_next_runnable_task_skips_rejected_dep() {
    let mut t1 = task("RQ-0002");
    t1.status = TaskStatus::Todo;
    t1.depends_on = vec!["RQ-0001".to_string()];

    let mut t_rejected = task("RQ-0001");
    t_rejected.status = TaskStatus::Rejected;
    t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let queue = QueueFile {
        version: 1,
        tasks: vec![t1],
    };
    let done_queue = QueueFile {
        version: 1,
        tasks: vec![t_rejected],
    };

    // Strict check: Rejected is NOT Done, so it should be skipped
    let next = next_runnable_task(&queue, Some(&done_queue));
    assert!(next.is_none());
}

#[test]
fn archive_done_tasks_moves_done_and_rejected() -> anyhow::Result<()> {
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

    let report = archive_done_tasks(&queue_path, &done_path, "RQ", 4)?;

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
fn archive_done_tasks_stamps_missing_completed_at() -> anyhow::Result<()> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");

    // Terminal task missing completed_at (this is the bug scenario).
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

    let report = archive_done_tasks(&queue_path, &done_path, "RQ", 4)?;
    assert_eq!(report.moved_ids, vec!["RQ-0001".to_string()]);

    let done_content = std::fs::read_to_string(&done_path)?;
    let done: QueueFile = serde_json::from_str(&done_content)?;
    assert_eq!(done.tasks.len(), 1);

    let completed_at = done.tasks[0]
        .completed_at
        .as_deref()
        .expect("completed_at should be stamped");

    // Ensure it is RFC3339 parseable.
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::parse(completed_at, &Rfc3339).expect("completed_at must be RFC3339");

    Ok(())
}

#[test]
fn archive_done_tasks_backfills_existing_done_without_moves() -> anyhow::Result<()> {
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

    let report = archive_done_tasks(&queue_path, &done_path, "RQ", 4)?;
    assert!(report.moved_ids.is_empty());

    let done_content = std::fs::read_to_string(&done_path)?;
    let done: QueueFile = serde_json::from_str(&done_content)?;
    let completed_at = done.tasks[0]
        .completed_at
        .as_deref()
        .expect("completed_at should be backfilled");

    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::parse(completed_at, &Rfc3339).expect("completed_at must be RFC3339");

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
    assert_eq!(done_archived.updated_at.as_deref(), Some(now));
    assert_eq!(done_archived.completed_at.as_deref(), Some(now));

    let rejected_archived = done
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("RQ-0002 archived");
    assert_eq!(rejected_archived.updated_at.as_deref(), Some(now));
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
