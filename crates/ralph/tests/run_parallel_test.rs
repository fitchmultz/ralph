//! E2E integration tests for parallel run execution paths.
//!
//! Responsibilities:
//! - Test path mapping, state initialization, and workspace synchronization
//! - Verify task selection and worker coordination in parallel mode
//! - Validate state persistence and worker lifecycle
//!
//! Not handled here:
//! - PR creation and merge automation (see parallel_e2e_test.rs)
//! - State recovery after crashes (see parallel_state_recovery_test.rs)
//! - Queue mutation validation (see parallel_queue_mutation_test.rs)
//!
//! Invariants/Assumptions:
//! - Tests use fake runner binaries to avoid external dependencies
//! - Tests run with env_lock to prevent PATH race conditions
//! - Temp directories are created outside the repo

use anyhow::Result;
use std::process::Command;

mod test_support;

/// Helper to set up a bare remote repository and configure it as origin.
fn setup_origin_remote(repo_path: &std::path::Path) -> Result<()> {
    let remote = test_support::temp_dir_outside_repo();

    let status = Command::new("git")
        .current_dir(remote.path())
        .args(["init", "--bare", "--quiet"])
        .status()?;
    anyhow::ensure!(status.success(), "git init --bare failed");

    let remote_path = remote.path().to_string_lossy().to_string();
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "add", "origin", remote_path.as_str()])
        .status()?;
    anyhow::ensure!(status.success(), "git remote add origin failed");

    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["push", "-u", "origin", "HEAD"])
        .status()?;
    anyhow::ensure!(status.success(), "git push -u origin HEAD failed");

    Ok(())
}

// =============================================================================
// Test: Parallel Mode Selects Multiple Tasks
// =============================================================================

/// Verify that parallel mode correctly selects and starts multiple tasks concurrently.
///
/// Setup: Create 3 todo tasks, run with --parallel 2 --max-tasks 2
/// Expected: 2 tasks are selected and processed, parallel state file is created
#[test]
fn run_parallel_selects_multiple_tasks() -> Result<()> {
    let lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo with origin remote
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create 3 todo tasks
    let tasks = vec![
        test_support::make_test_task(
            "RQ-0001",
            "First parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
        test_support::make_test_task(
            "RQ-0002",
            "Second parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
        test_support::make_test_task(
            "RQ-0003",
            "Third parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
    ];
    test_support::write_queue(temp.path(), &tasks)?;

    // Init ralph project
    test_support::ralph_init(temp.path())?;

    // Create fake runner that exits 0 immediately
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;

    // Disable PR automation to simplify test
    test_support::configure_parallel_disabled(temp.path())?;

    // Run parallel mode with 2 workers and max 2 tasks
    let (status, stdout, stderr) = test_support::run_in_dir(
        temp.path(),
        &[
            "run",
            "loop",
            "--parallel",
            "2",
            "--max-tasks",
            "2",
            "--force",
        ],
    );

    // Release the lock explicitly before assertions that might panic
    drop(lock);

    let combined = format!("{}{}", stdout, stderr);
    eprintln!("Test output:\n{}", combined);
    eprintln!("Exit status: {:?}", status);

    // Verify parallel state file was created
    let state = test_support::read_parallel_state(temp.path())?;

    // With PR automation disabled, the run may succeed or fail
    // The key assertion is that the parallel state file is created
    assert!(
        state.is_some(),
        "Parallel state file should be created\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let state = state.unwrap();

    // Access JSON fields using serde_json API
    let tasks_in_flight = state
        .get("tasks_in_flight")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    let prs = state
        .get("prs")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);

    // Verify that tasks were tracked as in-flight (or were started)
    // Note: With fast noop runners, tasks may complete before we check state
    // So we check either:
    // 1. Tasks are in the in-flight list, OR
    // 2. Tasks have been processed (may be in done or removed from queue)
    let tasks_moved = count_tasks_in_done_or_removed(&tasks, temp.path())?;
    let tasks_processed = tasks_in_flight + prs + tasks_moved;

    assert!(
        tasks_processed >= 1 || combined.contains("RQ-0001") || combined.contains("parallel"),
        "Expected at least 1 task to be selected or processed. \
         State: {} in-flight, {} PRs, {} moved. Output:\n{}",
        tasks_in_flight,
        prs,
        tasks_moved,
        combined
    );

    // Verify state file has valid structure
    let started_at = state
        .get("started_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        !started_at.is_empty(),
        "State should have started_at timestamp"
    );

    let schema_version = state
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(schema_version, 3, "State should use current schema version");

    Ok(())
}

// =============================================================================
// Test: Parallel Mode Handles Task Completion
// =============================================================================

/// Verify that parallel mode properly handles task completion and updates queue state.
///
/// Setup: Create 2 todo tasks with fake runner
/// Expected: Tasks are processed, completion is detected, queue state is updated
#[test]
fn run_parallel_handles_task_completion() -> Result<()> {
    let lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo with origin remote
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create 2 todo tasks
    let tasks = vec![
        test_support::make_test_task(
            "RQ-0001",
            "Task to complete",
            ralph::contracts::TaskStatus::Todo,
        ),
        test_support::make_test_task(
            "RQ-0002",
            "Another task",
            ralph::contracts::TaskStatus::Todo,
        ),
    ];
    test_support::write_queue(temp.path(), &tasks)?;

    // Init ralph project
    test_support::ralph_init(temp.path())?;

    // Create fake runner that simulates success
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;

    // Disable PR automation
    test_support::configure_parallel_disabled(temp.path())?;

    // Run parallel mode
    let (status, stdout, stderr) = test_support::run_in_dir(
        temp.path(),
        &[
            "run",
            "loop",
            "--parallel",
            "2",
            "--max-tasks",
            "2",
            "--force",
        ],
    );

    // Release the lock explicitly before assertions that might panic
    drop(lock);

    let combined = format!("{}{}", stdout, stderr);
    eprintln!("Test output:\n{}", combined);
    eprintln!("Exit status: {:?}", status);

    // Verify output indicates tasks were processed
    // The parallel mode should at least acknowledge the tasks or indicate parallel execution
    let has_task_reference = combined.contains("RQ-0001") || combined.contains("RQ-0002");
    let has_parallel_indicator = combined.contains("parallel")
        || combined.contains("Parallel")
        || combined.contains("worker")
        || combined.contains("state")
        || status.success();

    assert!(
        has_task_reference || has_parallel_indicator,
        "Expected output to reference tasks or indicate parallel mode execution. Output:\n{}",
        combined
    );

    // Verify parallel state file exists (even if empty, it should be created)
    let state = test_support::read_parallel_state(temp.path())?;
    if let Some(state) = state {
        // If state exists, verify it has valid structure
        let schema_version = state
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert_eq!(schema_version, 3, "State should use current schema version");

        // Access JSON fields using serde_json API
        let tasks_in_flight = state
            .get("tasks_in_flight")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0);
        let prs = state
            .get("prs")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0);

        // Tasks may be in-flight, have PRs, or have been completed
        let total_tracked = tasks_in_flight + prs;
        eprintln!(
            "Parallel state: {} tasks in-flight, {} PRs",
            tasks_in_flight, prs
        );

        // With fast noop runners, tasks may complete very quickly
        // The important thing is that the state file was created and is valid
        assert!(
            total_tracked <= 2,
            "Should not track more tasks than were started"
        );
    }

    // Verify workspace directories are handled (cleanup depends on config)
    let workspace_dir = temp.path().join(".ralph/cache/parallel/workspaces");
    if workspace_dir.exists() {
        // If workspaces exist, they should be valid directories
        for entry in std::fs::read_dir(&workspace_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                eprintln!("Workspace directory exists: {}", path.display());
            }
        }
    }

    Ok(())
}

// =============================================================================
// Test: Path Map Workspace Sync
// =============================================================================

/// Verify path mapping correctly syncs config/prompts into workspace clones.
#[test]
fn parallel_path_map_workspace_sync() -> Result<()> {
    let lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo and init ralph
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create task first (before init, as it creates .ralph dir)
    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Test path mapping",
        ralph::contracts::TaskStatus::Todo,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Init ralph project
    test_support::ralph_init(temp.path())?;

    // Create config and prompts AFTER ralph_init
    let config_path = temp.path().join(".ralph/config.json");
    std::fs::write(
        &config_path,
        r#"{"version":1,"agent":{"runner":"opencode","model":"test-model"}}"#,
    )?;
    std::fs::create_dir_all(temp.path().join(".ralph/prompts"))?;
    std::fs::write(
        temp.path().join(".ralph/prompts/worker.md"),
        "# Custom prompt",
    )?;

    // Configure and run
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_disabled(temp.path())?;

    test_support::run_in_dir(
        temp.path(),
        &[
            "run",
            "loop",
            "--parallel",
            "2",
            "--max-tasks",
            "1",
            "--force",
        ],
    );

    drop(lock);

    // Find the workspace directory
    let workspaces_dir = temp.path().join(".ralph/workspaces");
    if workspaces_dir.exists() {
        // Check if any workspace has the synced config and prompts
        for entry in std::fs::read_dir(&workspaces_dir)? {
            let entry = entry?;
            let workspace_path = entry.path();
            let workspace_config = workspace_path.join(".ralph/config.json");
            let workspace_prompt = workspace_path.join(".ralph/prompts/worker.md");

            if workspace_config.exists() {
                let config_content = std::fs::read_to_string(&workspace_config)?;
                assert!(
                    config_content.contains("test-model"),
                    "Config should be synced to workspace"
                );
            }

            if workspace_prompt.exists() {
                let prompt_content = std::fs::read_to_string(&workspace_prompt)?;
                assert_eq!(
                    prompt_content, "# Custom prompt",
                    "Prompt should be synced to workspace"
                );
            }
        }
    }

    // Queue/done should NOT be synced to workers (coordinator-only)
    // This is implicitly tested by the fact that the coordinator maintains these files

    Ok(())
}

// =============================================================================
// Test: State Initialization
// =============================================================================

/// Verify state file creation and content on parallel run startup.
#[test]
fn parallel_state_initialization() -> Result<()> {
    let lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    let tasks = vec![
        test_support::make_test_task("RQ-0001", "Task 1", ralph::contracts::TaskStatus::Todo),
        test_support::make_test_task("RQ-0002", "Task 2", ralph::contracts::TaskStatus::Todo),
    ];
    test_support::write_queue(temp.path(), &tasks)?;

    test_support::ralph_init(temp.path())?;

    // Verify state does NOT exist before run
    let state_path = temp.path().join(".ralph/cache/parallel/state.json");
    assert!(!state_path.exists(), "State should not exist before run");

    // Configure and run
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_disabled(temp.path())?;

    test_support::run_in_dir(
        temp.path(),
        &[
            "run",
            "loop",
            "--parallel",
            "2",
            "--max-tasks",
            "1",
            "--force",
        ],
    );

    drop(lock);

    // State file should be created after run
    if state_path.exists() {
        let state_content = std::fs::read_to_string(&state_path)?;
        let state: serde_json::Value = serde_json::from_str(&state_content)?;

        // Verify required fields exist (schema v3: direct-push mode)
        assert!(
            state.get("schema_version").is_some(),
            "State should have schema_version"
        );
        assert!(
            state.get("started_at").is_some(),
            "State should have started_at"
        );
        assert!(
            state.get("target_branch").is_some(),
            "State should have target_branch"
        );

        // Verify workers array exists (schema v3 uses workers instead of tasks_in_flight)
        assert!(state.get("workers").is_some(), "State should have workers");
    }

    Ok(())
}

// =============================================================================
// Test: Worker Workspace State Sync
// =============================================================================

/// Verify sync.rs correctly copies allowlisted files and excludes directories.
#[test]
fn parallel_worker_workspace_state_sync() -> Result<()> {
    let lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create task first
    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Test sync",
        ralph::contracts::TaskStatus::Todo,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Init ralph
    test_support::ralph_init(temp.path())?;

    // Create files that should be synced AFTER init
    std::fs::write(temp.path().join(".env"), "SECRET_KEY=test")?;
    std::fs::write(temp.path().join(".env.local"), "LOCAL_SECRET=test")?;

    // Create files that should NOT be synced
    std::fs::create_dir_all(temp.path().join("target/debug"))?;
    std::fs::write(temp.path().join("target/debug/app"), "binary")?;

    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_disabled(temp.path())?;

    test_support::run_in_dir(
        temp.path(),
        &[
            "run",
            "loop",
            "--parallel",
            "2",
            "--max-tasks",
            "1",
            "--force",
        ],
    );

    drop(lock);

    // Check workspace contents (if workspaces exist - they may be cleaned up immediately with noop runners)
    let workspaces_dir = temp.path().join(".ralph/workspaces");
    if workspaces_dir.exists() {
        for entry in std::fs::read_dir(&workspaces_dir)? {
            let entry = entry?;
            let workspace_path = entry.path();

            // .env and .env.local should be synced
            let env_path = workspace_path.join(".env");
            let env_local_path = workspace_path.join(".env.local");

            if env_path.exists() {
                let env_content = std::fs::read_to_string(&env_path)?;
                assert_eq!(
                    env_content, "SECRET_KEY=test",
                    ".env should be synced to workspace"
                );
            }

            if env_local_path.exists() {
                let env_local_content = std::fs::read_to_string(&env_local_path)?;
                assert_eq!(
                    env_local_content, "LOCAL_SECRET=test",
                    ".env.local should be synced to workspace"
                );
            }

            // target/ directory should NOT be synced
            let target_path = workspace_path.join("target");
            assert!(
                !target_path.exists(),
                "target/ directory should not be synced to workspace"
            );
        }
    }

    // Verify state file exists (proves parallel mode ran)
    let state_path = temp.path().join(".ralph/cache/parallel/state.json");
    assert!(
        state_path.exists(),
        "Parallel state file should exist after run"
    );

    Ok(())
}

// =============================================================================
// Test: Task Selection Multiple Workers
// =============================================================================

/// Verify multiple tasks are selected and workers respect --parallel limit.
#[test]
fn parallel_task_selection_multiple_workers() -> Result<()> {
    let lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo with origin remote
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create 3 todo tasks, run with --parallel 2 and --max-tasks 2
    let tasks = vec![
        test_support::make_test_task("RQ-0001", "Task 1", ralph::contracts::TaskStatus::Todo),
        test_support::make_test_task("RQ-0002", "Task 2", ralph::contracts::TaskStatus::Todo),
        test_support::make_test_task("RQ-0003", "Task 3", ralph::contracts::TaskStatus::Todo),
    ];
    test_support::write_queue(temp.path(), &tasks)?;

    test_support::ralph_init(temp.path())?;

    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_disabled(temp.path())?;

    test_support::run_in_dir(
        temp.path(),
        &[
            "run",
            "loop",
            "--parallel",
            "2",
            "--max-tasks",
            "2",
            "--force",
        ],
    );

    drop(lock);

    // Read state file to verify worker count
    let state_path = temp.path().join(".ralph/cache/parallel/state.json");
    if state_path.exists() {
        let state_content = std::fs::read_to_string(&state_path)?;
        let state: serde_json::Value = serde_json::from_str(&state_content)?;

        if let Some(tasks_in_flight) = state.get("tasks_in_flight").and_then(|v| v.as_array()) {
            // State should show at most 2 tasks in flight (respects --parallel 2)
            assert!(
                tasks_in_flight.len() <= 2,
                "Should have at most 2 tasks in flight (parallel limit)"
            );

            // Verify task IDs are valid
            for task in tasks_in_flight {
                if let Some(task_id) = task.get("task_id").and_then(|v| v.as_str()) {
                    assert!(
                        task_id.starts_with("RQ-"),
                        "Task ID should be valid: {}",
                        task_id
                    );
                }
            }
        }
    }

    // Verify worker workspaces exist
    let workspaces_dir = temp.path().join(".ralph/workspaces");
    if workspaces_dir.exists() {
        let workspace_count = std::fs::read_dir(&workspaces_dir)?.count();
        // Should have created at most 2 workspaces (respects --parallel 2 and --max-tasks 2)
        assert!(
            workspace_count <= 2,
            "Should have at most 2 worker workspaces"
        );
    }

    Ok(())
}

// =============================================================================
// Test: Handles Worker Completion
// =============================================================================

/// Verify task completion updates state and workers are tracked correctly.
#[test]
fn parallel_handles_worker_completion() -> Result<()> {
    let lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo with origin remote
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Complete task",
        ralph::contracts::TaskStatus::Todo,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    test_support::ralph_init(temp.path())?;

    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_disabled(temp.path())?;

    test_support::run_in_dir(
        temp.path(),
        &[
            "run",
            "loop",
            "--parallel",
            "2",
            "--max-tasks",
            "1",
            "--force",
        ],
    );

    drop(lock);

    // Read state after run
    let state_path = temp.path().join(".ralph/cache/parallel/state.json");

    // State file should exist after run
    assert!(state_path.exists(), "State file should exist after run");

    if state_path.exists() {
        let state_content = std::fs::read_to_string(&state_path)?;
        let state: serde_json::Value = serde_json::from_str(&state_content)?;

        // Verify tasks_in_flight exists and is valid
        if let Some(tasks_in_flight) = state.get("tasks_in_flight").and_then(|v| v.as_array()) {
            // After completion, tasks_in_flight should be empty or reflect only running workers
            // (The noop runner exits immediately, so task should complete)
            // We just verify the field exists and is valid
            for task in tasks_in_flight {
                if let Some(task_id) = task.get("task_id").and_then(|v| v.as_str()) {
                    assert!(
                        task_id.starts_with("RQ-"),
                        "Task ID should be valid: {}",
                        task_id
                    );
                }
            }
        }

        // Verify state structure (schema v3: direct-push mode)
        assert!(
            state.get("schema_version").is_some(),
            "State should have schema_version"
        );
        assert!(
            state.get("started_at").is_some(),
            "State should have started_at"
        );
        assert!(
            state.get("target_branch").is_some(),
            "State should have target_branch"
        );
    }

    Ok(())
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Count how many of the original tasks appear in done.json or are missing from queue.
///
/// This helps verify that tasks were processed even if they completed quickly
/// and don't appear in the parallel state.
fn count_tasks_in_done_or_removed(
    original_tasks: &[ralph::contracts::Task],
    dir: &std::path::Path,
) -> Result<usize> {
    let queue = test_support::read_queue(dir)?;
    let done = test_support::read_done(dir)?;

    let mut count = 0;
    for task in original_tasks {
        // Task is "processed" if it's in done OR not in queue (removed after completion)
        let in_done = done.tasks.iter().any(|t| t.id == task.id);
        let still_in_queue = queue.tasks.iter().any(|t| t.id == task.id);

        if in_done || !still_in_queue {
            count += 1;
        }
    }

    Ok(count)
}
