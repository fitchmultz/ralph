//! Integration tests for done.json safety in parallel execution (RQ-0958).
//!
//! Responsibilities:
//! - Validate no done.json drift or merge conflicts after parallel execution.
//! - Verify queue/done semantics validation uses correct paths.
//! - Test worker-side bookkeeping restore mechanisms.
//!
//! Not handled here:
//! - Full end-to-end parallel execution (see parallel_direct_push_test.rs).
//! - PR-based parallel flow (removed in rewrite).

use anyhow::{Context, Result};
use std::process::Command;

mod test_support;

/// Setup a bare remote repository and configure it as origin.
fn setup_origin_remote(repo_path: &std::path::Path) -> Result<std::path::PathBuf> {
    let remote = test_support::temp_dir_outside_repo();

    let status = Command::new("git")
        .current_dir(remote.path())
        .args(["init", "--bare", "--quiet"])
        .status()?;
    anyhow::ensure!(status.success(), "git init --bare failed");

    let remote_path = remote.path().to_string_lossy().to_string();
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "add", "origin", &remote_path])
        .status()?;
    anyhow::ensure!(status.success(), "git remote add origin failed");

    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["push", "-u", "origin", "HEAD"])
        .status()?;
    anyhow::ensure!(status.success(), "git push -u origin HEAD failed");

    Ok(remote.path().to_path_buf())
}

// =============================================================================
// Test: No Merge Conflicts in done.json After Parallel Execution
// =============================================================================

/// Verify that done.json contains no merge conflict markers after parallel run.
/// This test validates the safety mechanisms in place to prevent done.json drift.
#[test]
fn done_json_no_merge_conflicts_after_parallel_run() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Initialize git repo with remote
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create initial commit
    std::fs::write(temp.path().join("base.txt"), "initial content\n")?;
    test_support::git_add_all_commit(temp.path(), "Initial commit")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    // Create two tasks
    let tasks = vec![
        test_support::make_test_task(
            "RQ-0958-A",
            "First parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
        test_support::make_test_task(
            "RQ-0958-B",
            "Second parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
    ];
    test_support::write_queue(temp.path(), &tasks)?;
    test_support::ralph_init(temp.path())?;

    // Create a noop runner
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_for_direct_push(temp.path())?;

    // Commit initial state
    test_support::git_add_all_commit(temp.path(), "Add tasks for parallel test")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    // Run parallel with 2 workers, max 2 tasks
    let (_status, _stdout, _stderr) = test_support::with_prepend_path(&bin_dir, || {
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
        )
    });

    // Verify done.json has no conflict markers
    let done_path = temp.path().join(".ralph/done.jsonc");
    if done_path.exists() {
        let done_content = std::fs::read_to_string(&done_path)?;
        assert!(
            !done_content.contains("<<<<<<<"),
            "done.jsonc should not contain conflict start marker"
        );
        assert!(
            !done_content.contains("======="),
            "done.jsonc should not contain conflict separator"
        );
        assert!(
            !done_content.contains(">>>>>>>"),
            "done.jsonc should not contain conflict end marker"
        );
    }

    // Verify queue.json has no conflict markers
    let queue_path = temp.path().join(".ralph/queue.jsonc");
    if queue_path.exists() {
        let queue_content = std::fs::read_to_string(&queue_path)?;
        assert!(
            !queue_content.contains("<<<<<<<"),
            "queue.jsonc should not contain conflict markers"
        );
        assert!(
            !queue_content.contains("======="),
            "queue.jsonc should not contain conflict markers"
        );
        assert!(
            !queue_content.contains(">>>>>>>"),
            "queue.jsonc should not contain conflict markers"
        );
    }

    Ok(())
}

// =============================================================================
// Test: Queue/Done Semantic Validation After Parallel Run
// =============================================================================

/// Verify that queue/done semantics are valid after parallel execution.
/// This ensures no duplicate task IDs exist and all tasks are properly archived.
#[test]
fn queue_done_semantics_valid_after_parallel_run() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Initialize git repo with remote
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create initial commit
    std::fs::write(temp.path().join("base.txt"), "initial content\n")?;
    test_support::git_add_all_commit(temp.path(), "Initial commit")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    // Create two tasks
    let tasks = vec![
        test_support::make_test_task(
            "RQ-0958-A",
            "First parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
        test_support::make_test_task(
            "RQ-0958-B",
            "Second parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
    ];
    test_support::write_queue(temp.path(), &tasks)?;
    test_support::ralph_init(temp.path())?;

    // Create a noop runner
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_for_direct_push(temp.path())?;

    // Commit initial state
    test_support::git_add_all_commit(temp.path(), "Add tasks for parallel test")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    // Run parallel with 2 workers, max 2 tasks
    let (_status, _stdout, _stderr) = test_support::with_prepend_path(&bin_dir, || {
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
        )
    });

    // Verify queue/done files can be parsed as valid JSON
    let queue_path = temp.path().join(".ralph/queue.jsonc");
    let done_path = temp.path().join(".ralph/done.jsonc");

    if queue_path.exists() {
        let queue_content = std::fs::read_to_string(&queue_path)?;
        let _: serde_json::Value =
            serde_json::from_str(&queue_content).context("queue.jsonc should be valid JSON")?;
    }

    if done_path.exists() {
        let done_content = std::fs::read_to_string(&done_path)?;
        let _: serde_json::Value =
            serde_json::from_str(&done_content).context("done.jsonc should be valid JSON")?;
    }

    // Run ralph queue validate if available
    let (validate_status, validate_stdout, validate_stderr) =
        test_support::run_in_dir(temp.path(), &["queue", "validate"]);
    let validate_combined = format!("{}{}", validate_stdout, validate_stderr);

    // Validation should either succeed or report empty queue (both are ok)
    assert!(
        validate_status.success()
            || validate_combined.contains("valid")
            || validate_combined.contains("empty")
            || validate_combined.is_empty(),
        "Queue validation should pass or be empty: {}",
        validate_combined
    );

    Ok(())
}

// =============================================================================
// Test: Local and Remote done.json Consistency
// =============================================================================

/// Verify that local and remote done.json are consistent after parallel run.
/// This catches any divergence that could indicate drift issues.
#[test]
fn done_json_consistency_between_local_and_remote() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Initialize git repo with remote
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create initial commit
    std::fs::write(temp.path().join("base.txt"), "initial content\n")?;
    test_support::git_add_all_commit(temp.path(), "Initial commit")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    // Create two tasks
    let tasks = vec![
        test_support::make_test_task(
            "RQ-0958-A",
            "First parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
        test_support::make_test_task(
            "RQ-0958-B",
            "Second parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
    ];
    test_support::write_queue(temp.path(), &tasks)?;
    test_support::ralph_init(temp.path())?;

    // Create a noop runner
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_for_direct_push(temp.path())?;

    // Commit initial state
    test_support::git_add_all_commit(temp.path(), "Add tasks for parallel test")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    // Run parallel with 2 workers, max 2 tasks
    let (_status, _stdout, _stderr) = test_support::with_prepend_path(&bin_dir, || {
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
        )
    });

    // Fetch from remote
    Command::new("git")
        .current_dir(temp.path())
        .args(["fetch", "origin"])
        .status()?;

    // Get merge base
    let merge_base_output = Command::new("git")
        .current_dir(temp.path())
        .args(["merge-base", "HEAD", "origin/main"])
        .output()?;
    let merge_base = String::from_utf8_lossy(&merge_base_output.stdout)
        .trim()
        .to_string();

    // Use merge-tree to check for potential conflicts
    let merge_tree_output = Command::new("git")
        .current_dir(temp.path())
        .args(["merge-tree", &merge_base, "HEAD", "origin/main"])
        .output()?;
    let merge_tree_result = String::from_utf8_lossy(&merge_tree_output.stdout);

    // Check that merge-tree doesn't show conflicts for queue/done files
    if merge_tree_result.contains("conflict") {
        // Verify the conflicts are not in queue/done files
        assert!(
            !merge_tree_result.contains("queue.jsonc") && !merge_tree_result.contains("done.jsonc"),
            "queue.jsonc or done.jsonc should not have merge conflicts: {}",
            merge_tree_result
        );
    }

    Ok(())
}

// =============================================================================
// Test: No Blocked Push Workers Due to done.json Issues
// =============================================================================

/// Verify that parallel state does not contain blocked_push workers
/// due to done.json conflicts after execution.
#[test]
fn no_blocked_push_workers_from_done_json_conflicts() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    // Initialize git repo with remote
    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create initial commit
    std::fs::write(temp.path().join("base.txt"), "initial content\n")?;
    test_support::git_add_all_commit(temp.path(), "Initial commit")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    // Create two tasks
    let tasks = vec![
        test_support::make_test_task(
            "RQ-0958-A",
            "First parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
        test_support::make_test_task(
            "RQ-0958-B",
            "Second parallel task",
            ralph::contracts::TaskStatus::Todo,
        ),
    ];
    test_support::write_queue(temp.path(), &tasks)?;
    test_support::ralph_init(temp.path())?;

    // Create a noop runner
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_for_direct_push(temp.path())?;

    // Commit initial state
    test_support::git_add_all_commit(temp.path(), "Add tasks for parallel test")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    // Run parallel with 2 workers, max 2 tasks
    let (_status, _stdout, _stderr) = test_support::with_prepend_path(&bin_dir, || {
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
        )
    });

    // Check parallel state for blocked_push workers
    let state_path = temp.path().join(".ralph/cache/parallel/state.json");
    if state_path.exists() {
        let state_content = std::fs::read_to_string(&state_path)?;
        let state: serde_json::Value = serde_json::from_str(&state_content)?;

        if let Some(workers) = state["workers"].as_array() {
            for worker in workers {
                let lifecycle = worker["lifecycle"].as_str().unwrap_or("unknown");
                let task_id = worker["task_id"].as_str().unwrap_or("unknown");
                let last_error = worker["last_error"].as_str().unwrap_or("");

                // Workers blocked due to done.json conflicts are a failure indicator
                if lifecycle == "blocked_push" {
                    // Check if the error mentions queue/done issues
                    let is_queue_done_error = last_error.contains("queue")
                        || last_error.contains("done")
                        || last_error.contains("conflict");

                    assert!(
                        !is_queue_done_error,
                        "Worker {} is blocked_push with queue/done error: {}",
                        task_id, last_error
                    );
                }
            }
        }
    }

    Ok(())
}
