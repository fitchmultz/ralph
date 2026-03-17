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
//! - Tests prefer explicit fake runner binary paths when they need local runners.
//! - Nested `ralph run loop --parallel ...` invocations hold `parallel_run_lock()` only for the overlapping run window.
//! - Temp directories are created outside the repo and use disposable cached scaffolding.

use anyhow::{Context, Result};

#[path = "parallel_direct_push_test/support.rs"]
mod support;
mod test_support;

#[test]
fn parallel_status_empty_state() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::new()?;

    let (status, stdout, stderr) = repo.run(&["run", "parallel", "status"]);
    let combined = format!("{stdout}{stderr}");

    assert!(
        status.success() || combined.contains("No parallel run state found"),
        "Should succeed or show empty state message: {combined}"
    );

    Ok(())
}

#[test]
fn parallel_status_json_output() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::new()?;

    let (status, stdout, _stderr) = repo.run(&["run", "parallel", "status", "--json"]);
    assert!(status.success(), "JSON status should succeed");

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

#[test]
fn parallel_retry_no_state_fails() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::new()?;

    let (status, _stdout, stderr) = repo.run(&["run", "parallel", "retry", "--task", "RQ-0001"]);

    assert!(!status.success(), "Retry should fail without state");
    assert!(
        stderr.contains("No parallel run state found") || stderr.contains("not found"),
        "Should show appropriate error: {stderr}"
    );

    Ok(())
}

#[test]
fn parallel_worker_lifecycle_transitions() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::with_origin()?;
    repo.write_queue(&support::todo_tasks(&[("RQ-0001", "Test lifecycle")]))?;
    repo.configure_default_runner()?;

    let _ = repo.run_parallel(1);

    if let Some(state) = repo.read_parallel_state()? {
        assert_eq!(state["schema_version"].as_u64(), Some(3));
        assert!(state["target_branch"].is_string());
        assert!(state["workers"].is_array());

        if let Some(workers) = state["workers"].as_array() {
            for worker in workers {
                assert!(
                    worker["lifecycle"].is_string(),
                    "Worker should have lifecycle field"
                );
                let lifecycle = worker["lifecycle"].as_str().unwrap_or("unknown");
                assert!(
                    [
                        "running",
                        "integrating",
                        "completed",
                        "failed",
                        "blocked_push"
                    ]
                    .contains(&lifecycle),
                    "Invalid lifecycle: {lifecycle}"
                );
            }
        }
    }

    Ok(())
}

#[test]
fn parallel_retry_blocked_worker() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::new()?;
    repo.write_parallel_state(&serde_json::json!({
        "schema_version": 3,
        "started_at": "2026-02-20T00:00:00Z",
        "target_branch": "main",
        "workers": [{
            "task_id": "RQ-0001",
            "workspace_path": test_support::portable_abs_path("ws/RQ-0001"),
            "lifecycle": "blocked_push",
            "started_at": "2026-02-20T00:00:00Z",
            "completed_at": null,
            "push_attempts": 5,
            "last_error": "Max attempts exhausted"
        }]
    }))?;

    let (status, stdout, stderr) = repo.run(&["run", "parallel", "retry", "--task", "RQ-0001"]);
    let combined = format!("{stdout}{stderr}");

    assert!(status.success(), "Retry should succeed: {combined}");
    assert!(
        combined.contains("marked for retry") || combined.contains("retry"),
        "Should indicate retry scheduled"
    );

    let state = repo.read_parallel_state_required()?;
    let worker = &state["workers"][0];
    assert_eq!(worker["lifecycle"], "running");
    assert!(worker["last_error"].is_null());

    Ok(())
}

#[test]
fn parallel_retry_completed_worker_fails() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::new()?;
    repo.write_parallel_state(&serde_json::json!({
        "schema_version": 3,
        "started_at": "2026-02-20T00:00:00Z",
        "target_branch": "main",
        "workers": [{
            "task_id": "RQ-0001",
            "workspace_path": test_support::portable_abs_path("ws/RQ-0001"),
            "lifecycle": "completed",
            "started_at": "2026-02-20T00:00:00Z",
            "completed_at": "2026-02-20T01:00:00Z",
            "push_attempts": 1,
            "last_error": null
        }]
    }))?;

    let (status, _stdout, stderr) = repo.run(&["run", "parallel", "retry", "--task", "RQ-0001"]);

    assert!(!status.success(), "Retry should fail for completed worker");
    assert!(
        stderr.contains("already completed") || stderr.contains("completed successfully"),
        "Should indicate already completed: {stderr}"
    );

    Ok(())
}

#[test]
fn parallel_state_schema_v3_structure() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::with_origin()?;
    repo.write_queue(&support::todo_tasks(&[("RQ-0001", "Test schema")]))?;
    repo.configure_default_runner()?;

    let _ = repo.run_parallel(1);

    if let Some(state) = repo.read_parallel_state()? {
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

#[test]
fn parallel_worker_success_with_modifications() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::with_origin()?;
    repo.write_relative_file("data.txt", "initial\n")?;
    test_support::git_add_all_commit(repo.path(), "Initial data")?;
    repo.push_origin_head()?;

    repo.write_queue(&support::todo_tasks(&[("RQ-0001", "Modify data file")]))?;
    repo.configure_runner_script(
        r#"#!/bin/bash
echo "modified by worker" > data.txt
exit 0
"#,
    )?;

    let (_status, stdout, stderr) = repo.run_parallel(1);
    let combined = format!("{stdout}{stderr}");
    eprintln!("Parallel run output:\n{combined}");

    if let Some(state) = repo.read_parallel_state()?
        && let Some(workers) = state["workers"].as_array()
    {
        for worker in workers {
            let lifecycle = worker["lifecycle"].as_str().unwrap_or("unknown");
            assert!(
                ["completed", "failed", "blocked_push", "integrating"].contains(&lifecycle),
                "Worker should be in a terminal or late-stage state, got: {lifecycle}"
            );
        }
    }

    Ok(())
}

#[test]
fn parallel_multiple_tasks_execution() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::with_origin()?;
    repo.write_queue(&support::todo_tasks(&[
        ("RQ-0001", "First task"),
        ("RQ-0002", "Second task"),
    ]))?;
    repo.configure_default_runner()?;

    let (_status, stdout, stderr) = repo.run_parallel(2);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("parallel")
            || combined.contains("RQ-0001")
            || combined.contains("worker")
            || repo.state_path().exists(),
        "Should indicate parallel execution: {combined}"
    );

    Ok(())
}

#[test]
fn parallel_state_v2_to_v3_migration() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::new()?;
    repo.write_parallel_state(&serde_json::json!({
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
    }))?;

    let (status, stdout, _stderr) = repo.run(&["run", "parallel", "status", "--json"]);
    assert!(status.success(), "Status should succeed after migration");

    let state: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(
        state["schema_version"].as_u64(),
        Some(3),
        "Should be migrated to v3"
    );

    Ok(())
}

#[test]
fn parallel_status_shows_correct_summary() -> Result<()> {
    let repo = support::ParallelDirectPushRepo::new()?;
    repo.write_parallel_state(&serde_json::json!({
        "schema_version": 3,
        "started_at": "2026-02-20T00:00:00Z",
        "target_branch": "main",
        "workers": [
            {
                "task_id": "RQ-0001",
                "workspace_path": test_support::portable_abs_path("ws1"),
                "lifecycle": "completed",
                "started_at": "2026-02-20T00:00:00Z",
                "completed_at": "2026-02-20T01:00:00Z",
                "push_attempts": 1,
                "last_error": null
            },
            {
                "task_id": "RQ-0002",
                "workspace_path": test_support::portable_abs_path("ws2"),
                "lifecycle": "failed",
                "started_at": "2026-02-20T00:00:00Z",
                "completed_at": "2026-02-20T01:00:00Z",
                "push_attempts": 3,
                "last_error": "Some error"
            },
            {
                "task_id": "RQ-0003",
                "workspace_path": test_support::portable_abs_path("ws3"),
                "lifecycle": "blocked_push",
                "started_at": "2026-02-20T00:00:00Z",
                "completed_at": null,
                "push_attempts": 5,
                "last_error": "Max retries"
            }
        ]
    }))?;

    let (status, stdout, _stderr) = repo.run(&["run", "parallel", "status"]);
    assert!(status.success(), "Status should succeed");

    assert!(
        stdout.contains("Completed: 1") || stdout.contains("completed"),
        "Should show 1 completed: {stdout}"
    );
    assert!(
        stdout.contains("Failed: 1") || stdout.contains("failed"),
        "Should show 1 failed: {stdout}"
    );
    assert!(
        stdout.contains("Blocked: 1") || stdout.contains("blocked"),
        "Should show 1 blocked: {stdout}"
    );

    Ok(())
}
