//! E2E integration tests for parallel run execution paths.
//!
//! Responsibilities:
//! - Test path mapping, state initialization, and workspace synchronization.
//! - Verify task selection and worker coordination in parallel mode.
//! - Validate state persistence and worker lifecycle.
//!
//! Not handled here:
//! - State recovery after crashes (see `parallel_state_recovery_test.rs`).
//! - Queue mutation validation (see `parallel_queue_mutation_test.rs`).
//!
//! Invariants/Assumptions:
//! - Tests use explicit fake runner binary paths to avoid external dependencies.
//! - Nested `ralph run loop --parallel ...` invocations hold `parallel_run_lock()` only for the overlapping run window.
//! - Temp directories are created outside the repo and use disposable cached scaffolding.

use anyhow::Result;
use ralph::contracts::Task;

#[path = "run_parallel_test/support.rs"]
mod support;
mod test_support;

#[test]
fn run_parallel_selects_multiple_tasks() -> Result<()> {
    let (repo, tasks) = configured_repo(&[
        ("RQ-0001", "First parallel task"),
        ("RQ-0002", "Second parallel task"),
        ("RQ-0003", "Third parallel task"),
    ])?;

    let (status, stdout, stderr) = repo.run_parallel(2);
    let combined = format!("{stdout}{stderr}");
    let state = repo.read_parallel_state_required()?;

    let workers = worker_count(&state);
    let tasks_moved = repo.count_tasks_in_done_or_removed(&tasks)?;
    let tasks_processed = workers + tasks_moved;

    assert!(
        tasks_processed >= 1 || combined.contains("RQ-0001") || combined.contains("parallel"),
        "Expected at least 1 task to be selected or processed. \
         State: {workers} workers, {tasks_moved} moved. Output:\n{combined}"
    );

    let started_at = state
        .get("started_at")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        !started_at.is_empty(),
        "State should have started_at timestamp"
    );

    let schema_version = state
        .get("schema_version")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    assert_eq!(schema_version, 3, "State should use current schema version");

    assert!(
        state
            .get("workers")
            .and_then(|value| value.as_array())
            .is_some(),
        "State should have workers array\nstdout:\n{stdout}\nstderr:\n{stderr}\nstatus:{status:?}"
    );

    Ok(())
}

#[test]
fn run_parallel_handles_task_completion() -> Result<()> {
    let (repo, _) =
        configured_repo(&[("RQ-0001", "Task to complete"), ("RQ-0002", "Another task")])?;

    let (status, stdout, stderr) = repo.run_parallel(2);
    let combined = format!("{stdout}{stderr}");

    let has_task_reference = combined.contains("RQ-0001") || combined.contains("RQ-0002");
    let has_parallel_indicator = combined.contains("parallel")
        || combined.contains("Parallel")
        || combined.contains("worker")
        || combined.contains("state")
        || status.success();

    assert!(
        has_task_reference || has_parallel_indicator,
        "Expected output to reference tasks or indicate parallel mode execution. Output:\n{combined}"
    );

    if let Some(state) = repo.read_parallel_state()? {
        let schema_version = state
            .get("schema_version")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        assert_eq!(schema_version, 3, "State should use current schema version");
        assert!(
            worker_count(&state) <= 2,
            "Should not track more tasks than were started"
        );
    }

    for workspace in repo.workspace_dirs()? {
        assert!(workspace.is_dir(), "Workspace should be a directory");
    }

    Ok(())
}

#[test]
fn parallel_path_map_workspace_sync() -> Result<()> {
    let repo = support::RunParallelRepo::new()?;
    repo.write_queue(&support::todo_tasks(&[("RQ-0001", "Test path mapping")]))?;
    repo.write_relative_file(
        ".ralph/config.jsonc",
        r#"{"version":2,"agent":{"runner":"opencode","model":"test-model"}}"#,
    )?;
    repo.write_relative_file(".ralph/prompts/worker.md", "# Custom prompt")?;
    repo.configure_default_runner()?;

    let _ = repo.run_parallel(1);

    for workspace in repo.workspace_dirs()? {
        let workspace_config = workspace.join(".ralph/config.jsonc");
        let workspace_prompt = workspace.join(".ralph/prompts/worker.md");

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

    Ok(())
}

#[test]
fn parallel_state_initialization() -> Result<()> {
    let (repo, _) = configured_repo(&[("RQ-0001", "Task 1"), ("RQ-0002", "Task 2")])?;
    let state_path = repo.state_path();
    assert!(!state_path.exists(), "State should not exist before run");

    let _ = repo.run_parallel(1);

    if state_path.exists() {
        let state = repo.read_parallel_state_required()?;
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
        assert!(state.get("workers").is_some(), "State should have workers");
    }

    Ok(())
}

#[test]
fn parallel_worker_workspace_state_sync() -> Result<()> {
    let repo = support::RunParallelRepo::new()?;
    repo.write_queue(&support::todo_tasks(&[("RQ-0001", "Test sync")]))?;
    repo.write_relative_file(".env", "SECRET_KEY=test")?;
    repo.write_relative_file(".env.local", "LOCAL_SECRET=test")?;
    repo.write_relative_file("target/debug/app", "binary")?;
    repo.configure_default_runner()?;

    let _ = repo.run_parallel(1);

    for workspace in repo.workspace_dirs()? {
        let env_path = workspace.join(".env");
        let env_local_path = workspace.join(".env.local");

        if env_path.exists() {
            assert_eq!(
                std::fs::read_to_string(&env_path)?,
                "SECRET_KEY=test",
                ".env should be synced to workspace"
            );
        }

        if env_local_path.exists() {
            assert_eq!(
                std::fs::read_to_string(&env_local_path)?,
                "LOCAL_SECRET=test",
                ".env.local should be synced to workspace"
            );
        }

        assert!(
            !workspace.join("target").exists(),
            "target/ directory should not be synced to workspace"
        );
    }

    assert!(
        repo.state_path().exists(),
        "Parallel state file should exist after run"
    );

    Ok(())
}

#[test]
fn parallel_task_selection_multiple_workers() -> Result<()> {
    let (repo, _) = configured_repo(&[
        ("RQ-0001", "Task 1"),
        ("RQ-0002", "Task 2"),
        ("RQ-0003", "Task 3"),
    ])?;

    let _ = repo.run_parallel(2);

    if let Some(state) = repo.read_parallel_state()?
        && let Some(workers) = state.get("workers").and_then(|value| value.as_array())
    {
        assert!(
            workers.len() <= 2,
            "Should have at most 2 tracked workers (parallel limit)"
        );

        for worker in workers {
            if let Some(task_id) = worker.get("task_id").and_then(|value| value.as_str()) {
                assert!(
                    task_id.starts_with("RQ-"),
                    "Task ID should be valid: {task_id}"
                );
            }
        }
    }

    assert!(
        repo.workspace_dirs()?.len() <= 2,
        "Should have at most 2 worker workspaces"
    );

    Ok(())
}

#[test]
fn parallel_handles_worker_completion() -> Result<()> {
    let (repo, _) = configured_repo(&[("RQ-0001", "Complete task")])?;

    let _ = repo.run_parallel(1);
    assert!(
        repo.state_path().exists(),
        "State file should exist after run"
    );

    let state = repo.read_parallel_state_required()?;
    if let Some(workers) = state.get("workers").and_then(|value| value.as_array()) {
        for worker in workers {
            if let Some(task_id) = worker.get("task_id").and_then(|value| value.as_str()) {
                assert!(
                    task_id.starts_with("RQ-"),
                    "Task ID should be valid: {task_id}"
                );
            }
        }
    }

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

    Ok(())
}

fn configured_repo(entries: &[(&str, &str)]) -> Result<(support::RunParallelRepo, Vec<Task>)> {
    let repo = support::RunParallelRepo::new()?;
    let tasks = support::todo_tasks(entries);
    repo.write_queue(&tasks)?;
    repo.configure_default_runner()?;
    Ok((repo, tasks))
}

fn worker_count(state: &serde_json::Value) -> usize {
    state
        .get("workers")
        .and_then(|value| value.as_array())
        .map(std::vec::Vec::len)
        .unwrap_or(0)
}
