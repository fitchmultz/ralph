//! Worker queue-repair and validation selection tests.

use super::*;

#[test]
fn select_next_task_locked_uses_done_file_for_dependency_resolution() -> Result<()> {
    use crate::config;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let coordinator_dir = repo_root.join("coordinator");
    std::fs::create_dir_all(&coordinator_dir)?;

    let queue_path = coordinator_dir.join("queue.json");
    let done_path = coordinator_dir.join("done.json");

    let mut queue_file = QueueFile::default();
    queue_file.tasks.push(Task {
        id: "RQ-0002".to_string(),
        title: "Blocked by dependency".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec!["RQ-0001".to_string()],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    });
    queue::save_queue(&queue_path, &queue_file)?;

    let mut done_file = QueueFile::default();
    done_file.tasks.push(Task {
        id: "RQ-0001".to_string(),
        title: "Completed dependency".to_string(),
        description: None,
        status: TaskStatus::Done,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: Some("2026-01-02T00:00:00Z".to_string()),
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    });
    queue::save_queue(&done_path, &done_file)?;

    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;
    let excluded = HashSet::new();
    let result = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?;

    assert_eq!(
        result,
        Some(("RQ-0002".to_string(), "Blocked by dependency".to_string()))
    );
    Ok(())
}

#[test]
fn select_next_task_locked_rejects_non_utc_done_timestamps_without_persisting() -> Result<()> {
    use crate::config;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let mut queue_file = QueueFile::default();
    queue_file.tasks.push(Task {
        id: "RQ-0002".to_string(),
        title: "Ready task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec!["RQ-0001".to_string()],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    });
    queue::save_queue(&queue_path, &queue_file)?;

    let mut done_file = QueueFile::default();
    done_file.tasks.push(Task {
        id: "RQ-0001".to_string(),
        title: "Completed dependency".to_string(),
        description: None,
        status: TaskStatus::Done,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-02-22T17:34:44-07:00".to_string()),
        updated_at: Some("2026-02-22T17:34:44-07:00".to_string()),
        completed_at: Some("2026-02-22T17:34:44-07:00".to_string()),
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    });
    queue::save_queue(&done_path, &done_file)?;

    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path,
        done_path: done_path.clone(),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;
    let excluded = HashSet::new();
    let err = select_next_task_locked(&resolved, false, &excluded, &queue_lock)
        .expect_err("selection should stay read-only and reject repairable timestamps");
    let err_msg = format!("{err:#}");
    assert!(
        err_msg.contains("Parallel worker selection is read-only"),
        "error should explain read-only selection repair guidance: {err_msg}"
    );
    assert!(
        err_msg.contains("ralph queue repair"),
        "error should point to undo-backed repair: {err_msg}"
    );

    let persisted_done = queue::load_queue(&done_path)?;
    let completed = persisted_done.tasks[0]
        .completed_at
        .as_deref()
        .expect("completed_at should remain set");
    assert_eq!(completed, "2026-02-22T17:34:44-07:00");

    Ok(())
}

#[test]
fn parallel_select_next_task_locked_repairs_trailing_commas() -> Result<()> {
    use crate::config;
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let malformed = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test task", "status": "todo", "tags": ["bug",], "scope": ["file",], "evidence": ["observed",], "plan": ["do thing",], "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",}]}"#;
    std::fs::write(&queue_path, malformed)?;

    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path,
        done_path: ralph_dir.join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;
    let excluded = HashSet::new();
    let result = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?;

    assert!(result.is_some());
    let (task_id, task_title) = result.unwrap();
    assert_eq!(task_id, "RQ-0001");
    assert_eq!(task_title, "Test task");

    Ok(())
}

#[test]
fn parallel_select_next_task_locked_rejects_semantically_invalid_queue() -> Result<()> {
    use crate::config;
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    // Intentionally missing created_at / updated_at (should fail semantic validation).
    let invalid = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test task", "status": "todo", "tags": ["bug"], "scope": ["file"], "evidence": [], "plan": []}]}"#;
    std::fs::write(&queue_path, invalid)?;

    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path,
        done_path: ralph_dir.join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;
    let excluded = HashSet::new();

    let err = select_next_task_locked(&resolved, false, &excluded, &queue_lock)
        .expect_err("expected semantic validation failure");
    let err_msg = err
        .chain()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        err_msg.contains("created_at") || err_msg.contains("updated_at"),
        "error should mention missing timestamps: {err_msg}"
    );

    Ok(())
}
