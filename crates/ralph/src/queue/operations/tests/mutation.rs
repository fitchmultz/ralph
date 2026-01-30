//! Tests for `mutation.rs` operations (queue-level collection mutations).

use super::*;

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
    let now_canon = canonical_rfc3339(now);
    let updated = backfill_terminal_completed_at(&mut queue, now_canon.as_str());
    assert_eq!(updated, 2);

    assert_eq!(
        queue.tasks[0].completed_at.as_deref(),
        Some(now_canon.as_str())
    );
    assert_eq!(
        queue.tasks[1].completed_at.as_deref(),
        Some(now_canon.as_str())
    );
    assert_eq!(
        queue.tasks[2].completed_at.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );

    Ok(())
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
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
        }],
    };
    let now_canon = canonical_rfc3339("2026-01-18T00:00:00Z");
    backfill_missing_fields(
        &mut queue,
        &["RQ-0002".to_string()],
        "req",
        now_canon.as_str(),
    );
    let task = &queue.tasks[0];
    assert_eq!(task.request.as_deref(), Some("req"));
    assert_eq!(task.created_at.as_deref(), Some(now_canon.as_str()));
    assert_eq!(task.updated_at.as_deref(), Some(now_canon.as_str()));
}

#[test]
fn backfill_missing_fields_populates_request() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    queue.tasks[0].request = None;

    let now_canon = canonical_rfc3339("2026-01-18T12:34:56Z");
    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string()],
        "default request",
        now_canon.as_str(),
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

    let now_canon = canonical_rfc3339("2026-01-18T12:34:56Z");
    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string()],
        "default request",
        now_canon.as_str(),
    );

    assert_eq!(queue.tasks[0].created_at, Some(now_canon.clone()));
    assert_eq!(queue.tasks[0].updated_at, Some(now_canon));
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

    let now_canon = canonical_rfc3339("2026-01-18T12:34:56Z");
    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string()],
        "default request",
        now_canon.as_str(),
    );

    assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
    assert_eq!(queue.tasks[0].created_at, Some(now_canon.clone()));
    assert_eq!(queue.tasks[0].updated_at, Some(now_canon));
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
fn backfill_missing_fields_empty_new_task_ids_noops() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    queue.tasks[0].request = None;
    queue.tasks[0].created_at = None;
    queue.tasks[0].updated_at = None;

    backfill_missing_fields(&mut queue, &[], "default request", "2026-01-18T12:34:56Z");

    assert_eq!(queue.tasks[0].request, None);
    assert_eq!(queue.tasks[0].created_at, None);
    assert_eq!(queue.tasks[0].updated_at, None);
}

#[test]
fn backfill_missing_fields_handles_duplicate_new_task_ids() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    queue.tasks[0].request = None;
    queue.tasks[0].created_at = None;
    queue.tasks[0].updated_at = None;

    let now_canon = canonical_rfc3339("2026-01-18T12:34:56Z");
    backfill_missing_fields(
        &mut queue,
        &["RQ-0001".to_string(), "RQ-0001".to_string()],
        "default request",
        now_canon.as_str(),
    );

    assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
    assert_eq!(queue.tasks[0].created_at, Some(now_canon.clone()));
    assert_eq!(queue.tasks[0].updated_at, Some(now_canon));
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
                scheduled_start: None,
                depends_on: vec![],
                blocks: vec![],
                relates_to: vec![],
                duplicates: None,
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
                scheduled_start: None,
                depends_on: vec![],
                blocks: vec![],
                relates_to: vec![],
                duplicates: None,
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
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
    });

    let ids = task_id_set(&queue);
    assert_eq!(ids.len(), 1);
    assert!(ids.contains("RQ-0001"));
}

#[test]
fn suggest_new_task_insert_index_empty_queue_is_zero() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    assert_eq!(suggest_new_task_insert_index(&queue), 0);
}

#[test]
fn suggest_new_task_insert_index_first_doing_is_one() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let mut doing = task_with("RQ-0001", TaskStatus::Doing, vec!["code".to_string()]);
    doing.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    queue.tasks.push(doing);
    assert_eq!(suggest_new_task_insert_index(&queue), 1);
}

#[test]
fn suggest_new_task_insert_index_first_not_doing_is_zero() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let doing = task_with("RQ-0001", TaskStatus::Todo, vec!["code".to_string()]);
    queue.tasks.push(doing);
    assert_eq!(suggest_new_task_insert_index(&queue), 0);
}

#[test]
fn suggest_new_task_insert_index_first_done_is_zero() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let mut done = task_with("RQ-0001", TaskStatus::Done, vec!["code".to_string()]);
    done.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    queue.tasks.push(done);
    assert_eq!(suggest_new_task_insert_index(&queue), 0);
}

#[test]
fn reposition_new_tasks_inserts_at_top_when_insert_at_zero() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0002"), task("RQ-0003")],
    };
    let mut new_task = task("RQ-0004");
    new_task.title = "New Task".to_string();
    queue.tasks.push(new_task);
    let new_ids = vec!["RQ-0004".to_string()];

    reposition_new_tasks(&mut queue, &new_ids, 0);

    assert_eq!(queue.tasks[0].id, "RQ-0004");
    assert_eq!(queue.tasks[0].title, "New Task");
    assert_eq!(queue.tasks[1].id, "RQ-0001");
    assert_eq!(queue.tasks[2].id, "RQ-0002");
    assert_eq!(queue.tasks[3].id, "RQ-0003");
}

#[test]
fn reposition_new_tasks_inserts_after_first_when_insert_at_one() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0002"), task("RQ-0003")],
    };
    let mut new_task = task("RQ-0004");
    new_task.title = "New Task".to_string();
    queue.tasks.push(new_task);
    let new_ids = vec!["RQ-0004".to_string()];

    reposition_new_tasks(&mut queue, &new_ids, 1);

    assert_eq!(queue.tasks[0].id, "RQ-0001");
    assert_eq!(queue.tasks[1].id, "RQ-0004");
    assert_eq!(queue.tasks[1].title, "New Task");
    assert_eq!(queue.tasks[2].id, "RQ-0002");
    assert_eq!(queue.tasks[3].id, "RQ-0003");
}

#[test]
fn reposition_new_tasks_preserves_multiple_new_task_order() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0002"), task("RQ-0003")],
    };
    let mut task_a = task("RQ-0004");
    task_a.title = "Task A".to_string();
    let mut task_b = task("RQ-0005");
    task_b.title = "Task B".to_string();
    queue.tasks.push(task_a);
    queue.tasks.push(task_b);

    let new_ids = vec!["RQ-0004".to_string(), "RQ-0005".to_string()];

    reposition_new_tasks(&mut queue, &new_ids, 1);

    assert_eq!(queue.tasks[0].id, "RQ-0001");
    assert_eq!(queue.tasks[1].id, "RQ-0004");
    assert_eq!(queue.tasks[1].title, "Task A");
    assert_eq!(queue.tasks[2].id, "RQ-0005");
    assert_eq!(queue.tasks[2].title, "Task B");
    assert_eq!(queue.tasks[3].id, "RQ-0002");
    assert_eq!(queue.tasks[4].id, "RQ-0003");
}

#[test]
fn reposition_new_tasks_clamps_insert_index() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let mut new_task = task("RQ-0002");
    new_task.title = "New Task".to_string();
    queue.tasks.push(new_task);
    let new_ids = vec!["RQ-0002".to_string()];

    reposition_new_tasks(&mut queue, &new_ids, 999);

    assert_eq!(queue.tasks[0].id, "RQ-0001");
    assert_eq!(queue.tasks[1].id, "RQ-0002");
    assert_eq!(queue.tasks[1].title, "New Task");
}

#[test]
fn reposition_new_tasks_handles_empty_new_ids() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0002")],
    };

    let original_ids: Vec<_> = queue.tasks.iter().map(|t| t.id.clone()).collect();
    reposition_new_tasks(&mut queue, &[], 1);

    let after_ids: Vec<_> = queue.tasks.iter().map(|t| t.id.clone()).collect();
    assert_eq!(original_ids, after_ids);
}

#[test]
fn reposition_new_tasks_handles_empty_queue() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![],
    };

    reposition_new_tasks(&mut queue, &["RQ-0001".to_string()], 0);

    assert_eq!(queue.tasks.len(), 0);
}

#[test]
fn clone_task_creates_copy_with_new_id() {
    use crate::queue::operations::CloneTaskOptions;

    let mut source = task_with("RQ-0001", TaskStatus::Todo, vec!["code".to_string()]);
    source.title = "Source Task".to_string();
    source.priority = TaskPriority::High;
    source.scope = vec!["crates/ralph".to_string()];
    source.evidence = vec!["evidence".to_string()];
    source.plan = vec!["step 1".to_string()];
    source.notes = vec!["note".to_string()];
    source.request = Some("original request".to_string());
    // Note: depends_on is cleared during clone, so we don't set it here
    source
        .custom_fields
        .insert("key".to_string(), "value".to_string());

    let mut queue = QueueFile {
        version: 1,
        tasks: vec![source],
    };

    let now = "2026-01-20T12:00:00Z";
    let opts = CloneTaskOptions::new("RQ-0001", TaskStatus::Draft, now, "RQ", 4);
    let (new_id, cloned) = clone_task(&mut queue, None, &opts).unwrap();

    assert_eq!(new_id, "RQ-0002");
    assert_eq!(cloned.id, "RQ-0002");
    assert_eq!(cloned.title, "Source Task");
    assert_eq!(cloned.status, TaskStatus::Draft);
    assert_eq!(cloned.priority, TaskPriority::High);
    assert_eq!(cloned.tags, vec!["code".to_string()]);
    assert_eq!(cloned.scope, vec!["crates/ralph".to_string()]);
    assert_eq!(cloned.evidence, vec!["evidence".to_string()]);
    assert_eq!(cloned.plan, vec!["step 1".to_string()]);
    assert_eq!(cloned.notes, vec!["note".to_string()]);
    assert_eq!(cloned.request, Some("original request".to_string()));
    assert!(cloned.depends_on.is_empty()); // Dependencies cleared
    assert_eq!(cloned.custom_fields.get("key"), Some(&"value".to_string()));
    assert_eq!(cloned.created_at, Some(now.to_string()));
    assert_eq!(cloned.updated_at, Some(now.to_string()));
    assert_eq!(cloned.completed_at, None);
}

#[test]
fn clone_task_applies_title_prefix() {
    use crate::queue::operations::CloneTaskOptions;

    let source = task_with("RQ-0001", TaskStatus::Todo, vec![]);
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![source],
    };

    let opts = CloneTaskOptions::new(
        "RQ-0001",
        TaskStatus::Draft,
        "2026-01-20T12:00:00Z",
        "RQ",
        4,
    )
    .with_title_prefix(Some("[Clone] "));
    let (new_id, cloned) = clone_task(&mut queue, None, &opts).unwrap();

    assert_eq!(new_id, "RQ-0002");
    assert_eq!(cloned.title, "[Clone] Test task");
}

#[test]
fn clone_task_uses_custom_status() {
    use crate::queue::operations::CloneTaskOptions;

    // Use Todo status for source to avoid validation issues with Done tasks
    let source = task_with("RQ-0001", TaskStatus::Todo, vec![]);
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![source],
    };

    // Clone with Todo status instead of default Draft
    let opts = CloneTaskOptions::new("RQ-0001", TaskStatus::Todo, "2026-01-20T12:00:00Z", "RQ", 4);
    let (_, cloned) = clone_task(&mut queue, None, &opts).unwrap();

    assert_eq!(cloned.status, TaskStatus::Todo);
}

#[test]
fn clone_task_finds_source_in_done_file() {
    use crate::queue::operations::CloneTaskOptions;

    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };

    // Use Done status with completed_at for done.json (required by validation)
    let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec![]);
    done_task.title = "Done Task".to_string();
    done_task.completed_at = Some("2026-01-19T12:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };

    let opts = CloneTaskOptions::new(
        "RQ-0001",
        TaskStatus::Draft,
        "2026-01-20T12:00:00Z",
        "RQ",
        4,
    );
    let (new_id, cloned) = clone_task(&mut queue.clone(), Some(&done), &opts).unwrap();

    assert_eq!(new_id, "RQ-0002");
    assert_eq!(cloned.title, "Done Task");
}

#[test]
fn clone_task_errors_when_source_not_found() {
    use crate::queue::operations::CloneTaskOptions;

    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };

    let opts = CloneTaskOptions::new(
        "RQ-9999",
        TaskStatus::Draft,
        "2026-01-20T12:00:00Z",
        "RQ",
        4,
    );
    let result = clone_task(&mut queue.clone(), None, &opts);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"));
}
