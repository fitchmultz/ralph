//! Worker queue-order and blocked-push selection tests.
//!
//! Purpose:
//! - Worker queue-order and blocked-push selection tests.
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
fn select_next_task_locked_preserves_queue_order_with_terminal_workers() -> Result<()> {
    use crate::config;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
    use crate::queue;
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let mut queue_file = QueueFile::default();

    // Add tasks in non-ID order: RQ-0003 first, RQ-0001 second, RQ-0002 third
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
    queue_file.tasks.push(Task {
        id: "RQ-0002".to_string(),
        title: "Second ID, third in file".to_string(),
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

    // Create state file with terminal workers for RQ-0003 (Completed) and RQ-0001 (Failed)
    // These should NOT be excluded from selection - terminal state doesn't block re-selection
    let state_path = ralph_dir.join("cache/parallel/state.json");
    let mut state =
        state::ParallelStateFile::new("2026-01-01T00:00:00Z".to_string(), "main".to_string());

    // Completed worker for RQ-0003 (should NOT block selection)
    let mut completed_worker = state::WorkerRecord::new(
        "RQ-0003",
        crate::testsupport::path::portable_abs_path("workspace/RQ-0003"),
        "2026-01-01T00:00:00Z".to_string(),
    );
    completed_worker.mark_completed("2026-01-01T01:00:00Z".to_string());
    state.upsert_worker(completed_worker);

    // Failed worker for RQ-0001 (should NOT block selection)
    let mut failed_worker = state::WorkerRecord::new(
        "RQ-0001",
        crate::testsupport::path::portable_abs_path("workspace/RQ-0001"),
        "2026-01-01T00:00:00Z".to_string(),
    );
    failed_worker.mark_failed("2026-01-01T01:00:00Z".to_string(), "test error");
    state.upsert_worker(failed_worker);

    std::fs::create_dir_all(state_path.parent().unwrap())?;
    state::save_state(&state_path, &state)?;

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

    // Collect excluded IDs - terminal workers should NOT be excluded
    let in_flight: HashMap<String, WorkerState> = HashMap::new();
    let attempted: HashSet<String> = HashSet::new();
    let excluded = collect_excluded_ids(&state, &in_flight, &attempted);

    // Terminal workers should NOT be in excluded set
    assert!(
        !excluded.contains("RQ-0003"),
        "Completed worker should NOT be excluded"
    );
    assert!(
        !excluded.contains("RQ-0001"),
        "Failed worker should NOT be excluded"
    );

    let selected = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?
        .expect("a task should be selected");

    // Should select RQ-0003 (first in queue file order), NOT RQ-0001 (lowest ID)
    assert_eq!(
        selected.0, "RQ-0003",
        "parallel selection must honor queue file order even with terminal workers in state"
    );
    Ok(())
}

#[test]
fn select_next_task_locked_excludes_blocked_push_workers() -> Result<()> {
    use crate::config;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
    use crate::queue;
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let mut queue_file = QueueFile::default();

    // Add tasks: RQ-0001 first (will be blocked), RQ-0002 second
    queue_file.tasks.push(Task {
        id: "RQ-0001".to_string(),
        title: "First task (blocked)".to_string(),
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
        id: "RQ-0002".to_string(),
        title: "Second task".to_string(),
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

    // Create state with BlockedPush worker for RQ-0001 (SHOULD block selection)
    let state_path = ralph_dir.join("cache/parallel/state.json");
    let mut state =
        state::ParallelStateFile::new("2026-01-01T00:00:00Z".to_string(), "main".to_string());
    let mut blocked_worker = state::WorkerRecord::new(
        "RQ-0001",
        crate::testsupport::path::portable_abs_path("workspace/RQ-0001"),
        "2026-01-01T00:00:00Z".to_string(),
    );
    blocked_worker.mark_blocked("2026-01-01T01:00:00Z".to_string(), "merge conflict");
    state.upsert_worker(blocked_worker);

    std::fs::create_dir_all(state_path.parent().unwrap())?;
    state::save_state(&state_path, &state)?;

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

    let in_flight: HashMap<String, WorkerState> = HashMap::new();
    let attempted: HashSet<String> = HashSet::new();
    let excluded = collect_excluded_ids(&state, &in_flight, &attempted);

    // BlockedPush worker SHOULD be excluded
    assert!(
        excluded.contains("RQ-0001"),
        "BlockedPush worker should be excluded"
    );

    let selected = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?
        .expect("a task should be selected");

    // Should skip RQ-0001 (blocked) and select RQ-0002
    assert_eq!(
        selected.0, "RQ-0002",
        "parallel selection should skip blocked workers and select next in queue order"
    );
    Ok(())
}
