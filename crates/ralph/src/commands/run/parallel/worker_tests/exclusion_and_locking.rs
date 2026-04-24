//! Worker exclusion and lock-held selection tests.
//!
//! Purpose:
//! - Worker exclusion and lock-held selection tests.
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
fn collect_excluded_ids_excludes_in_flight_attempted_and_blocked_workers() -> Result<()> {
    let mut state_file =
        state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());
    let workspace_root = crate::testsupport::path::portable_abs_path("workspace");

    // Running worker (should be selectable for explicit retry flows)
    let running_worker = state::WorkerRecord::new(
        "RQ-0001",
        workspace_root.join("RQ-0001"),
        "2026-02-20T00:00:00Z".to_string(),
    );
    state_file.upsert_worker(running_worker);

    // Integrating worker (should be selectable; true active workers are tracked in-flight)
    let mut integrating_worker = state::WorkerRecord::new(
        "RQ-0002",
        workspace_root.join("RQ-0002"),
        "2026-02-20T00:00:00Z".to_string(),
    );
    integrating_worker.start_integration();
    state_file.upsert_worker(integrating_worker);

    // Completed worker (retained for status/reporting; not excluded by default)
    let mut completed_worker = state::WorkerRecord::new(
        "RQ-0003",
        workspace_root.join("RQ-0003"),
        "2026-02-20T00:00:00Z".to_string(),
    );
    completed_worker.mark_completed("2026-02-20T01:00:00Z".to_string());
    state_file.upsert_worker(completed_worker);

    // Failed worker (retained for status/retry; not excluded by default)
    let mut failed_worker = state::WorkerRecord::new(
        "RQ-0004",
        workspace_root.join("RQ-0004"),
        "2026-02-20T00:00:00Z".to_string(),
    );
    failed_worker.mark_failed("2026-02-20T01:00:00Z".to_string(), "error");
    state_file.upsert_worker(failed_worker);

    // Blocked worker (must stay excluded until explicit retry)
    let mut blocked_worker = state::WorkerRecord::new(
        "RQ-0006",
        workspace_root.join("RQ-0006"),
        "2026-02-20T00:00:00Z".to_string(),
    );
    blocked_worker.mark_blocked("2026-02-20T01:00:00Z".to_string(), "blocked");
    state_file.upsert_worker(blocked_worker);

    let mut in_flight = HashMap::new();
    let child = std::process::Command::new("true").spawn()?;
    let (worker_events_tx, _worker_events_rx) = std::sync::mpsc::channel();
    in_flight.insert(
        "RQ-0005".to_string(),
        start_worker_monitor(
            "RQ-0005",
            "title".to_string(),
            WorkspaceSpec {
                path: crate::testsupport::path::portable_abs_path("workspaces/RQ-0005"),
                branch: "main".to_string(),
            },
            child,
            worker_events_tx,
        ),
    );

    let mut attempted_in_run = HashSet::new();
    attempted_in_run.insert("RQ-0007".to_string());

    let excluded = collect_excluded_ids(&state_file, &in_flight, &attempted_in_run);

    // In-flight worker should be excluded
    assert!(
        excluded.contains("RQ-0005"),
        "in-flight worker should be excluded"
    );

    // Non-terminal state records should not be excluded.
    assert!(
        !excluded.contains("RQ-0001"),
        "running worker should NOT be excluded"
    );
    assert!(
        !excluded.contains("RQ-0002"),
        "integrating worker should NOT be excluded"
    );

    // Completed/failed workers are retained for status/retry but should not
    // block queue-ordered scheduling by default.
    assert!(
        !excluded.contains("RQ-0003"),
        "completed worker should NOT be excluded"
    );
    assert!(
        !excluded.contains("RQ-0004"),
        "failed worker should NOT be excluded"
    );
    assert!(
        excluded.contains("RQ-0006"),
        "blocked worker should be excluded"
    );
    assert!(
        excluded.contains("RQ-0007"),
        "attempted task should be excluded for this invocation"
    );

    terminate_workers(&mut in_flight);

    Ok(())
}

#[test]
fn select_next_task_locked_works_under_held_lock() -> Result<()> {
    use crate::config;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    // Create a queue with one todo task
    let queue_path = ralph_dir.join("queue.json");
    let mut queue_file = QueueFile::default();
    queue_file.tasks.push(Task {
        id: "RQ-0001".to_string(),
        title: "Test task".to_string(),
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
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    });
    queue::save_queue(&queue_path, &queue_file)?;

    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path: queue_path.clone(),
        done_path: ralph_dir.join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    // Acquire the queue lock (as the parallel supervisor would)
    let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;

    // Call select_next_task_locked with the held lock
    let excluded = HashSet::new();
    let result = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?;

    // Should return the todo task
    assert!(result.is_some());
    let (task_id, task_title) = result.unwrap();
    assert_eq!(task_id, "RQ-0001");
    assert_eq!(task_title, "Test task");

    Ok(())
}

#[test]
fn select_next_task_locked_returns_none_when_no_tasks() -> Result<()> {
    use crate::config;
    use crate::contracts::QueueFile;
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    // Create an empty queue
    let queue_path = ralph_dir.join("queue.json");
    let queue_file = QueueFile::default();
    queue::save_queue(&queue_path, &queue_file)?;

    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path: queue_path.clone(),
        done_path: ralph_dir.join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    // Acquire the queue lock
    let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;

    // Call select_next_task_locked with the held lock
    let excluded = HashSet::new();
    let result = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?;

    // Should return None since no tasks are available
    assert!(result.is_none());

    Ok(())
}

#[test]
fn select_next_task_locked_preserves_queue_order_over_task_id() -> Result<()> {
    use crate::config;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let mut queue_file = QueueFile::default();
    queue_file.tasks.push(Task {
        id: "RQ-0003".to_string(),
        title: "Third ID, first in file".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
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
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    });
    queue_file.tasks.push(Task {
        id: "RQ-0001".to_string(),
        title: "First ID, second in file".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
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
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    });
    queue::save_queue(&queue_path, &queue_file)?;

    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path: queue_path.clone(),
        done_path: ralph_dir.join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;
    let excluded = HashSet::new();
    let selected = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?
        .expect("a task should be selected");

    assert_eq!(
        selected.0, "RQ-0003",
        "parallel selection must honor queue file order, not task ID sort order"
    );
    Ok(())
}
