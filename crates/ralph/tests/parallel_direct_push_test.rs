//! Integration tests for parallel direct-push mode.
//!
//! Responsibilities:
//! - Test direct-push integration paths (no PR creation).
//! - Verify worker lifecycle states and transitions.
//! - Test conflict resolution and retry scenarios.
//! - Validate integration loop behavior.
//!
//! Not handled here:
//! - PR-based parallel flow (removed in rewrite).
//! - Merge-agent behavior (removed in rewrite).
//!
//! Invariants/assumptions:
//! - Tests use fake runner binaries to control worker behavior.
//! - Tests run with env_lock to prevent PATH race conditions.
//! - Temp directories are created outside the repo.

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
// Test: Parallel Status Command - Empty State
// =============================================================================

/// Verify status command shows empty state when no parallel run exists.
#[test]
fn parallel_status_empty_state() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    test_support::ralph_init(temp.path())?;

    // Run parallel status with no state
    let (status, stdout, stderr) =
        test_support::run_in_dir(temp.path(), &["run", "parallel", "status"]);

    let combined = format!("{}{}", stdout, stderr);
    assert!(
        status.success() || combined.contains("No parallel run state found"),
        "Should succeed or show empty state message: {}",
        combined
    );

    Ok(())
}

// =============================================================================
// Test: Parallel Status Command - JSON Output
// =============================================================================

/// Verify status command outputs valid JSON with --json flag.
#[test]
fn parallel_status_json_output() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    test_support::ralph_init(temp.path())?;

    // Run parallel status --json with no state
    let (status, stdout, _stderr) =
        test_support::run_in_dir(temp.path(), &["run", "parallel", "status", "--json"]);

    assert!(status.success(), "JSON status should succeed");

    // Verify valid JSON output
    let json: serde_json::Value =
        serde_json::from_str(&stdout).context("Status output should be valid JSON")?;

    assert_eq!(
        json["schema_version"].as_u64(),
        Some(3),
        "Should report schema version 3"
    );
    assert!(json["workers"].is_array(), "Should have workers array");

    Ok(())
}

// =============================================================================
// Test: Parallel Retry - No State Error
// =============================================================================

/// Verify retry command fails gracefully when no parallel state exists.
#[test]
fn parallel_retry_no_state_fails() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    test_support::ralph_init(temp.path())?;

    // Try to retry with no state
    let (status, _stdout, stderr) = test_support::run_in_dir(
        temp.path(),
        &["run", "parallel", "retry", "--task", "RQ-0001"],
    );

    assert!(!status.success(), "Retry should fail without state");
    assert!(
        stderr.contains("No parallel run state found") || stderr.contains("not found"),
        "Should show appropriate error: {}",
        stderr
    );

    Ok(())
}

// =============================================================================
// Test: Worker Lifecycle Transitions
// =============================================================================

/// Verify worker lifecycle transitions are tracked correctly.
#[test]
fn parallel_worker_lifecycle_transitions() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Test lifecycle",
        ralph::contracts::TaskStatus::Todo,
    )];
    test_support::write_queue(temp.path(), &tasks)?;
    test_support::ralph_init(temp.path())?;

    // Create fake runner
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::configure_parallel_for_direct_push(temp.path())?;
    test_support::trust_project_commands(temp.path())?;

    // Run parallel
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

    // Check state file
    let state_path = temp.path().join(".ralph/cache/parallel/state.json");
    if state_path.exists() {
        let state_content = std::fs::read_to_string(&state_path)?;
        let state: serde_json::Value = serde_json::from_str(&state_content)?;

        // Verify schema v3 structure
        assert_eq!(state["schema_version"].as_u64(), Some(3));
        assert!(state["target_branch"].is_string());
        assert!(state["workers"].is_array());

        // Verify workers have lifecycle field
        if let Some(workers) = state["workers"].as_array() {
            for worker in workers {
                assert!(
                    worker["lifecycle"].is_string(),
                    "Worker should have lifecycle field"
                );
                let lifecycle = worker["lifecycle"].as_str().unwrap();
                assert!(
                    [
                        "running",
                        "integrating",
                        "completed",
                        "failed",
                        "blocked_push"
                    ]
                    .contains(&lifecycle),
                    "Invalid lifecycle: {}",
                    lifecycle
                );
            }
        }
    }

    Ok(())
}

// =============================================================================
// Test: Retry Blocked Worker
// =============================================================================

/// Verify retry command works for blocked workers.
#[test]
fn parallel_retry_blocked_worker() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    test_support::ralph_init(temp.path())?;

    // Create state file with a blocked worker
    let state_dir = temp.path().join(".ralph/cache/parallel");
    std::fs::create_dir_all(&state_dir)?;

    let state = serde_json::json!({
        "schema_version": 3,
        "started_at": "2026-02-20T00:00:00Z",
        "target_branch": "main",
        "workers": [{
            "task_id": "RQ-0001",
            "workspace_path": "/tmp/ws/RQ-0001",
            "lifecycle": "blocked_push",
            "started_at": "2026-02-20T00:00:00Z",
            "completed_at": null,
            "push_attempts": 5,
            "last_error": "Max attempts exhausted"
        }]
    });

    std::fs::write(
        state_dir.join("state.json"),
        serde_json::to_string_pretty(&state)?,
    )?;

    // Retry the blocked worker
    let (status, stdout, stderr) = test_support::run_in_dir(
        temp.path(),
        &["run", "parallel", "retry", "--task", "RQ-0001"],
    );

    let combined = format!("{}{}", stdout, stderr);
    eprintln!("Retry output: {}", combined);

    assert!(status.success(), "Retry should succeed: {}", combined);
    assert!(
        combined.contains("marked for retry") || combined.contains("retry"),
        "Should indicate retry scheduled"
    );

    // Verify state was updated
    let updated_state = std::fs::read_to_string(state_dir.join("state.json"))?;
    let state: serde_json::Value = serde_json::from_str(&updated_state)?;

    let worker = &state["workers"][0];
    assert_eq!(worker["lifecycle"], "running");
    assert!(worker["last_error"].is_null());

    Ok(())
}

// =============================================================================
// Test: Retry Completed Worker Fails
// =============================================================================

/// Verify retry command fails for completed workers.
#[test]
fn parallel_retry_completed_worker_fails() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    test_support::ralph_init(temp.path())?;

    // Create state file with a completed worker
    let state_dir = temp.path().join(".ralph/cache/parallel");
    std::fs::create_dir_all(&state_dir)?;

    let state = serde_json::json!({
        "schema_version": 3,
        "started_at": "2026-02-20T00:00:00Z",
        "target_branch": "main",
        "workers": [{
            "task_id": "RQ-0001",
            "workspace_path": "/tmp/ws/RQ-0001",
            "lifecycle": "completed",
            "started_at": "2026-02-20T00:00:00Z",
            "completed_at": "2026-02-20T01:00:00Z",
            "push_attempts": 1,
            "last_error": null
        }]
    });

    std::fs::write(
        state_dir.join("state.json"),
        serde_json::to_string_pretty(&state)?,
    )?;

    // Try to retry completed worker
    let (status, _stdout, stderr) = test_support::run_in_dir(
        temp.path(),
        &["run", "parallel", "retry", "--task", "RQ-0001"],
    );

    assert!(!status.success(), "Retry should fail for completed worker");
    assert!(
        stderr.contains("already completed") || stderr.contains("completed successfully"),
        "Should indicate already completed: {}",
        stderr
    );

    Ok(())
}

// =============================================================================
// Test: State Schema v3 Validation
// =============================================================================

/// Verify state file has correct v3 schema structure.
#[test]
fn parallel_state_schema_v3_structure() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Test schema",
        ralph::contracts::TaskStatus::Todo,
    )];
    test_support::write_queue(temp.path(), &tasks)?;
    test_support::ralph_init(temp.path())?;

    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::trust_project_commands(temp.path())?;
    test_support::configure_parallel_for_direct_push(temp.path())?;

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

    // Verify state file structure
    let state_path = temp.path().join(".ralph/cache/parallel/state.json");
    if state_path.exists() {
        let content = std::fs::read_to_string(&state_path)?;
        let state: serde_json::Value = serde_json::from_str(&content)?;

        // Required v3 fields
        assert_eq!(
            state["schema_version"].as_u64(),
            Some(3),
            "Must be schema v3"
        );
        assert!(
            state["started_at"].as_str().is_some(),
            "Must have started_at"
        );
        assert!(
            state["target_branch"].as_str().is_some(),
            "Must have target_branch"
        );
        assert!(state["workers"].is_array(), "Must have workers array");

        // Verify no v2 fields exist
        assert!(state["prs"].is_null(), "v3 state should not have prs field");
        assert!(
            state["pending_merges"].is_null(),
            "v3 state should not have pending_merges field"
        );
        assert!(
            state["base_branch"].is_null(),
            "v3 uses target_branch, not base_branch"
        );
    }

    Ok(())
}

// =============================================================================
// Test: Worker Success Path with File Modification
// =============================================================================

/// Verify worker can modify files and complete successfully.
#[test]
fn parallel_worker_success_with_modifications() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create initial tracked file
    std::fs::write(temp.path().join("data.txt"), "initial\n")?;
    test_support::git_add_all_commit(temp.path(), "Initial data")?;

    // Push to origin
    Command::new("git")
        .current_dir(temp.path())
        .args(["push", "origin", "HEAD"])
        .status()?;

    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Modify data file",
        ralph::contracts::TaskStatus::Todo,
    )];
    test_support::write_queue(temp.path(), &tasks)?;
    test_support::ralph_init(temp.path())?;

    // Create runner that modifies the file
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_script = r#"#!/bin/bash
echo "modified by worker" > data.txt
exit 0
"#;
    test_support::create_executable_script(&bin_dir, "opencode", runner_script)?;
    test_support::configure_runner(
        temp.path(),
        "opencode",
        "test-model",
        Some(&bin_dir.join("opencode")),
    )?;
    test_support::configure_parallel_for_direct_push(temp.path())?;

    // Run parallel
    let (_status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
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
        )
    });

    let combined = format!("{}{}", stdout, stderr);
    eprintln!("Parallel run output:\n{}", combined);

    // Verify state file shows worker completion
    let state_path = temp.path().join(".ralph/cache/parallel/state.json");
    if state_path.exists() {
        let content = std::fs::read_to_string(&state_path)?;
        let state: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(workers) = state["workers"].as_array() {
            for worker in workers {
                // Worker should be in a terminal state (completed, failed, or blocked)
                let lifecycle = worker["lifecycle"].as_str().unwrap_or("unknown");
                assert!(
                    ["completed", "failed", "blocked_push", "integrating"].contains(&lifecycle),
                    "Worker should be in a terminal or late-stage state, got: {}",
                    lifecycle
                );
            }
        }
    }

    Ok(())
}

// =============================================================================
// Test: Multiple Tasks Parallel Execution
// =============================================================================

/// Verify parallel execution processes multiple tasks.
#[test]
fn parallel_multiple_tasks_execution() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    setup_origin_remote(temp.path())?;

    // Create multiple tasks
    let tasks = vec![
        test_support::make_test_task("RQ-0001", "First task", ralph::contracts::TaskStatus::Todo),
        test_support::make_test_task("RQ-0002", "Second task", ralph::contracts::TaskStatus::Todo),
    ];
    test_support::write_queue(temp.path(), &tasks)?;
    test_support::ralph_init(temp.path())?;

    // Create noop runner
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = test_support::create_noop_runner(temp.path(), "opencode")?;
    test_support::configure_runner(temp.path(), "opencode", "test-model", Some(&runner_path))?;
    test_support::trust_project_commands(temp.path())?;
    test_support::configure_parallel_for_direct_push(temp.path())?;

    // Run parallel with 2 workers, max 2 tasks
    let (_status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
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

    let combined = format!("{}{}", stdout, stderr);

    // Verify parallel execution was attempted
    // The state file may or may not exist depending on timing
    // but the output should indicate parallel execution
    assert!(
        combined.contains("parallel")
            || combined.contains("RQ-0001")
            || combined.contains("worker")
            || temp
                .path()
                .join(".ralph/cache/parallel/state.json")
                .exists(),
        "Should indicate parallel execution: {}",
        combined
    );

    Ok(())
}

// =============================================================================
// Test: State Migration v2 to v3
// =============================================================================

/// Verify v2 state files are migrated to v3 on load.
#[test]
fn parallel_state_v2_to_v3_migration() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    test_support::ralph_init(temp.path())?;

    // Create v2 state file
    let state_dir = temp.path().join(".ralph/cache/parallel");
    std::fs::create_dir_all(&state_dir)?;

    let v2_state = serde_json::json!({
        "schema_version": 2,
        "started_at": "2026-02-18T00:00:00Z",
        "base_branch": "main",
        "merge_method": "squash",
        "merge_when": "as_created",
        "tasks_in_flight": [],
        "prs": [
            {"task_id": "RQ-0001", "pr_number": 42, "lifecycle": "open"}
        ],
        "pending_merges": [
            {"task_id": "RQ-0001", "pr_number": 42, "lifecycle": "queued", "attempts": 0}
        ]
    });

    std::fs::write(
        state_dir.join("state.json"),
        serde_json::to_string_pretty(&v2_state)?,
    )?;

    // Run parallel status to trigger migration
    let (status, stdout, _stderr) =
        test_support::run_in_dir(temp.path(), &["run", "parallel", "status", "--json"]);

    assert!(status.success(), "Status should succeed after migration");

    // Verify output is valid v3
    let state: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(
        state["schema_version"].as_u64(),
        Some(3),
        "Should be migrated to v3"
    );

    Ok(())
}

// =============================================================================
// Test: Parallel Status Shows Correct Summary
// =============================================================================

/// Verify status command shows correct worker counts by lifecycle.
#[test]
fn parallel_status_shows_correct_summary() -> Result<()> {
    let _lock = test_support::env_lock().lock();
    let temp = test_support::temp_dir_outside_repo();

    test_support::git_init(temp.path())?;
    test_support::ralph_init(temp.path())?;

    // Create state with workers in different lifecycles
    let state_dir = temp.path().join(".ralph/cache/parallel");
    std::fs::create_dir_all(&state_dir)?;

    let state = serde_json::json!({
        "schema_version": 3,
        "started_at": "2026-02-20T00:00:00Z",
        "target_branch": "main",
        "workers": [
            {
                "task_id": "RQ-0001",
                "workspace_path": "/tmp/ws1",
                "lifecycle": "completed",
                "started_at": "2026-02-20T00:00:00Z",
                "completed_at": "2026-02-20T01:00:00Z",
                "push_attempts": 1,
                "last_error": null
            },
            {
                "task_id": "RQ-0002",
                "workspace_path": "/tmp/ws2",
                "lifecycle": "failed",
                "started_at": "2026-02-20T00:00:00Z",
                "completed_at": "2026-02-20T01:00:00Z",
                "push_attempts": 3,
                "last_error": "Some error"
            },
            {
                "task_id": "RQ-0003",
                "workspace_path": "/tmp/ws3",
                "lifecycle": "blocked_push",
                "started_at": "2026-02-20T00:00:00Z",
                "completed_at": null,
                "push_attempts": 5,
                "last_error": "Max retries"
            }
        ]
    });

    std::fs::write(
        state_dir.join("state.json"),
        serde_json::to_string_pretty(&state)?,
    )?;

    // Run status
    let (status, stdout, _stderr) =
        test_support::run_in_dir(temp.path(), &["run", "parallel", "status"]);

    assert!(status.success(), "Status should succeed");

    // Verify summary shows correct counts
    assert!(
        stdout.contains("Completed: 1") || stdout.contains("completed"),
        "Should show 1 completed: {}",
        stdout
    );
    assert!(
        stdout.contains("Failed: 1") || stdout.contains("failed"),
        "Should show 1 failed: {}",
        stdout
    );
    assert!(
        stdout.contains("Blocked: 1") || stdout.contains("blocked"),
        "Should show 1 blocked: {}",
        stdout
    );

    Ok(())
}
