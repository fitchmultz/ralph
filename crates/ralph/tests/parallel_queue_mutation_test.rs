//! Integration tests for parallel mode queue/done mutation paths.
//!
//! Responsibilities:
//! - Verify that queue/done mutations happen in coordinator context only.
//! - Test atomic write behavior (temp file + rename).
//! - Verify workspace queue files are not touched by coordinator operations.
//!
//! Not handled here:
//! - Full parallel orchestration (see parallel_e2e_test.rs).
//! - Merge-agent exit codes (see parallel_merge_agent_test.rs).
//! - State persistence (see parallel_state_recovery_test.rs).
//!
//! Invariants/assumptions:
//! - Coordinator queue is at `.ralph/queue.json` relative to repo root.
//! - Workspace queue (if any) should never be modified by coordinator.
//! - Tests use env_lock to prevent PATH race conditions.

use anyhow::Result;

mod test_support;

// =============================================================================
// Test: Coordinator Queue Is Mutated On Task Completion
// =============================================================================

/// Verify that merge-agent mutates the coordinator's queue.json, not workspace queue.
#[test]
fn merge_agent_mutates_coordinator_queue() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo (coordinator)
    test_support::git_init(temp.path())?;

    // Init ralph project first (creates queue.json)
    test_support::ralph_init(temp.path())?;

    // Create task in doing state (write AFTER ralph_init)
    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Task to complete",
        ralph::contracts::TaskStatus::Doing,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Create fake gh that returns merged PR
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent with PATH prepended
    let (_status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0001", "--pr", "42"],
        )
    });

    // Merge-agent should succeed
    eprintln!("Merge-agent output:\n{}{}", stdout, stderr);

    // Verify coordinator queue was updated (task removed or status changed)
    let coordinator_queue = test_support::read_queue(temp.path())?;
    let task_in_queue = coordinator_queue.tasks.iter().find(|t| t.id == "RQ-0001");

    // Task should either be removed from queue or moved to done
    let coordinator_done = test_support::read_done(temp.path())?;
    let task_in_done = coordinator_done.tasks.iter().find(|t| t.id == "RQ-0001");

    // At least one of these should be true for successful completion
    assert!(
        task_in_done.is_some()
            || task_in_queue.map(|t| t.status) == Some(ralph::contracts::TaskStatus::Done),
        "Task should be in done.json or marked done in queue"
    );

    Ok(())
}

// =============================================================================
// Test: Workspace Queue Is Not Mutated By Coordinator
// =============================================================================

/// Verify that a workspace-specific queue is not modified by coordinator operations.
#[test]
fn coordinator_does_not_mutate_workspace_queue() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let coordinator_dir = test_support::temp_dir_outside_repo();

    // Setup git repo (coordinator)
    test_support::git_init(coordinator_dir.path())?;

    // Init ralph project first
    test_support::ralph_init(coordinator_dir.path())?;

    // Create coordinator task in doing state (write AFTER ralph_init)
    let coordinator_tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Coordinator task",
        ralph::contracts::TaskStatus::Doing,
    )];
    test_support::write_queue(coordinator_dir.path(), &coordinator_tasks)?;

    // Create workspace directory with its own queue
    let workspace_dir = coordinator_dir.path().join("worktree-RQ-0002");
    std::fs::create_dir_all(&workspace_dir)?;
    std::fs::create_dir_all(workspace_dir.join(".ralph"))?;

    // Write workspace-specific queue (should be untouched)
    let workspace_tasks = vec![test_support::make_test_task(
        "RQ-0002",
        "Workspace task",
        ralph::contracts::TaskStatus::Todo,
    )];
    test_support::write_queue(&workspace_dir, &workspace_tasks)?;

    // Create fake gh that returns merged PR
    let bin_dir = coordinator_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent from coordinator context with PATH prepended
    let _ = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            coordinator_dir.path(),
            &["run", "merge-agent", "--task", "RQ-0001", "--pr", "42"],
        )
    });

    // Verify workspace queue is unchanged
    let workspace_queue = test_support::read_queue(&workspace_dir)?;
    assert_eq!(
        workspace_queue.tasks.len(),
        1,
        "Workspace queue should still have 1 task"
    );
    assert_eq!(workspace_queue.tasks[0].id, "RQ-0002");
    assert_eq!(
        workspace_queue.tasks[0].status,
        ralph::contracts::TaskStatus::Todo,
        "Workspace task status should be unchanged"
    );

    Ok(())
}

// =============================================================================
// Test: Queue File Is Not Corrupted On Partial Write
// =============================================================================

/// Verify that queue file remains valid after operations.
/// The atomic write (temp + rename) ensures no partial writes.
#[test]
fn queue_file_remains_valid_after_operations() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first
    test_support::ralph_init(temp.path())?;

    // Create multiple tasks (write AFTER ralph_init)
    let tasks = vec![
        test_support::make_test_task("RQ-0001", "Task 1", ralph::contracts::TaskStatus::Doing),
        test_support::make_test_task("RQ-0002", "Task 2", ralph::contracts::TaskStatus::Todo),
        test_support::make_test_task("RQ-0003", "Task 3", ralph::contracts::TaskStatus::Todo),
    ];
    test_support::write_queue(temp.path(), &tasks)?;

    // Verify queue is valid
    let queue = test_support::read_queue(temp.path())?;
    assert_eq!(queue.tasks.len(), 3);

    // Create fake gh that returns merged PR
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent to complete one task with PATH prepended
    let _ = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0001", "--pr", "42"],
        )
    });

    // Verify queue is still valid (can be parsed)
    let queue = test_support::read_queue(temp.path())?;
    assert!(
        queue.tasks.iter().all(|t| t.id.starts_with("RQ-")),
        "All task IDs should be valid"
    );

    // Verify done is still valid
    let done = test_support::read_done(temp.path())?;
    assert!(
        done.tasks.iter().all(|t| t.id.starts_with("RQ-")),
        "All done task IDs should be valid"
    );

    Ok(())
}

// =============================================================================
// Test: No Temp Files Left After Completion
// =============================================================================

/// Verify that no temp files are left after merge-agent completes.
#[test]
fn no_temp_files_after_merge_agent() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first
    test_support::ralph_init(temp.path())?;

    // Create task in doing state (write AFTER ralph_init)
    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Task to complete",
        ralph::contracts::TaskStatus::Doing,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Create fake gh that returns merged PR
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent with PATH prepended
    let _ = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0001", "--pr", "42"],
        )
    });

    // Check for temp files in .ralph directory
    let ralph_dir = temp.path().join(".ralph");
    if ralph_dir.exists() {
        for entry in std::fs::read_dir(&ralph_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            // Should not have .tmp, .bak, .partial, etc. files
            assert!(
                !name.ends_with(".tmp")
                    && !name.ends_with(".bak")
                    && !name.ends_with(".partial")
                    && !name.ends_with(".tmp~"),
                "Found temp file in .ralph directory: {}",
                name
            );
        }
    }

    Ok(())
}

// =============================================================================
// Test: Done File Is Created If Missing
// =============================================================================

/// Verify that done.json is created if it doesn't exist.
#[test]
fn done_file_created_if_missing() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first
    test_support::ralph_init(temp.path())?;

    // Create task in doing state (write AFTER ralph_init)
    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Task to complete",
        ralph::contracts::TaskStatus::Doing,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Delete done.json if it exists
    let done_path = temp.path().join(".ralph/done.json");
    if done_path.exists() {
        std::fs::remove_file(&done_path)?;
    }

    // Create fake gh that returns merged PR
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent with PATH prepended
    let _ = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0001", "--pr", "42"],
        )
    });

    // Verify done.json was created
    assert!(
        done_path.exists(),
        "done.json should be created after merge-agent completes"
    );

    // Verify it's valid JSON
    let done = test_support::read_done(temp.path())?;
    assert_eq!(done.version, 1);
    assert!(done.tasks.iter().any(|t| t.id == "RQ-0001"));

    Ok(())
}

// =============================================================================
// Test: Multiple Completions Don't Duplicate Entries
// =============================================================================

/// Verify that re-running merge-agent on the same task doesn't create duplicates.
#[test]
fn merge_agent_idempotent_no_duplicates() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first
    test_support::ralph_init(temp.path())?;

    // Create task already in done.json (write AFTER ralph_init)
    let done_task = test_support::make_test_task(
        "RQ-0001",
        "Already done task",
        ralph::contracts::TaskStatus::Done,
    );
    test_support::write_done(temp.path(), std::slice::from_ref(&done_task))?;

    // Empty queue (write AFTER ralph_init)
    test_support::write_queue(temp.path(), &[])?;

    // Create fake gh that returns merged PR
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent twice with PATH prepended
    test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0001", "--pr", "42"],
        )
    });
    test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0001", "--pr", "42"],
        )
    });

    // Verify only one entry in done
    let done = test_support::read_done(temp.path())?;
    let count = done.tasks.iter().filter(|t| t.id == "RQ-0001").count();
    assert_eq!(
        count, 1,
        "Should have exactly one entry for RQ-0001 in done.json"
    );

    Ok(())
}
