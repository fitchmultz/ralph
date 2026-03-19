//! Purpose: runner-backed lifecycle integration coverage.
//!
//! Responsibilities:
//! - Verify successful `ralph run one` auto-completes tasks when CI passes.
//! - Verify runner failures keep tasks in queue with Doing status.
//! - Verify queue and done-state tracking during runner execution.
//!
//! Scope:
//! - End-to-end runner integration scenarios using the suite-local fake runner bootstrap.
//!
//! Usage:
//! - Imported by `task_lifecycle_test.rs`; relies on `use super::*;` for shared fixtures and helpers.
//!
//! Invariants/assumptions callers must respect:
//! - Tests use `setup_runner_with_passing_ci()` to provision deterministic runner/CI behavior.
//! - Assertions and runner scripts remain unchanged from the pre-split suite.

use super::test_support;
use super::*;

/// Test full lifecycle with actual runner execution: create → ready → start → run → done.
///
/// This test verifies that:
/// 1. `ralph run one` selects a todo task and runs the runner.
/// 2. When CI gate passes, task is auto-completed.
/// 3. Task metadata (`started_at`, `completed_at`) is properly tracked.
#[test]
fn task_full_lifecycle_with_runner_execution() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task_id = "RQ-9001";
    let task = test_support::make_test_task(task_id, "Runner test task", TaskStatus::Todo);
    repo.write_queue(&[task])?;

    let marker_file = repo.path().join(".ralph/runner_executed.marker");
    let runner_script = format!(
        r#"#!/bin/bash
# Mock runner that verifies it received task context
echo "runner_executed" > "{}"
exit 0
"#,
        marker_file.display()
    );
    repo.setup_runner_with_passing_ci(&runner_script)?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist before run");
    assert_eq!(
        task.status,
        TaskStatus::Todo,
        "Initial: status should be Todo"
    );

    repo.run_ok(&["run", "one"])?;

    assert!(
        marker_file.exists(),
        "Runner should have been executed (marker file should exist)"
    );

    let queue = repo.read_queue()?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "Task should not be in queue after successful run + CI gate"
    );

    let done = repo.read_done()?;
    assert_eq!(done.tasks.len(), 1, "Task should be in done.json");
    let done_task = find_task(&done.tasks, task_id).expect("task should be in done.json");
    assert_eq!(done_task.status, TaskStatus::Done, "Status should be Done");
    assert!(done_task.started_at.is_some(), "started_at should be set");
    assert!(
        done_task.completed_at.is_some(),
        "completed_at should be set"
    );

    Ok(())
}

/// Test that runner failure prevents task auto-completion.
///
/// This test verifies that when the runner fails:
/// 1. Task remains in the queue.
/// 2. Task status is still Doing.
/// 3. User can then reject the task manually.
#[test]
fn task_runner_failure_prevents_auto_complete() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task_id = "RQ-9002";
    let task = test_support::make_test_task(task_id, "Task that will fail", TaskStatus::Todo);
    repo.write_queue(&[task])?;
    repo.setup_runner_with_passing_ci("#!/bin/sh\nexit 1\n")?;

    let (status, _, _stderr) = repo.run(&["run", "one"]);
    assert!(
        !status.success(),
        "Run should fail when runner exits with error"
    );

    let queue = repo.read_queue()?;
    assert_eq!(
        queue.tasks.len(),
        1,
        "Task should still be in queue after runner failure"
    );
    let task = find_task(&queue.tasks, task_id).expect("task should remain in queue");
    assert_eq!(
        task.status,
        TaskStatus::Doing,
        "Task should be Doing (set by run command)"
    );
    assert!(
        task.started_at.is_some(),
        "started_at should be set by run command"
    );

    repo.run_ok(&[
        "task",
        "reject",
        task_id,
        "--note",
        "Runner failed - won't fix",
    ])?;

    let queue = repo.read_queue()?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "Task should be removed from queue"
    );

    let done = repo.read_done()?;
    assert_eq!(done.tasks.len(), 1, "Task should be in done.json");
    let done_task = find_task(&done.tasks, task_id).expect("task should exist in done.json");
    assert_eq!(
        done_task.status,
        TaskStatus::Rejected,
        "Status should be Rejected"
    );
    assert!(
        done_task
            .notes
            .iter()
            .any(|note| note.contains("Runner failed")),
        "Reject note should be preserved"
    );

    Ok(())
}

/// Test queue state transitions during full lifecycle including runner execution.
///
/// This test verifies the complete state transition sequence with CI gate auto-completion:
/// 1. Initial: Task is Todo in queue.json.
/// 2. After `ralph run one`: Task is auto-completed and moved to done.json with Done status.
/// 3. Timestamps (`started_at`, `completed_at`) are properly set.
#[test]
fn task_lifecycle_queue_state_during_run() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task_id = "RQ-9003";
    let task = test_support::make_test_task(task_id, "State tracking task", TaskStatus::Todo);
    repo.write_queue(&[task])?;
    repo.setup_runner_with_passing_ci("#!/bin/sh\nexit 0\n")?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist before run");
    assert_eq!(
        task.status,
        TaskStatus::Todo,
        "Initial: status should be Todo"
    );
    assert!(
        task.started_at.is_none(),
        "Initial: started_at should not be set"
    );

    repo.run_ok(&["run", "one"])?;

    let queue = repo.read_queue()?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "After run: task should not be in queue (auto-completed by CI gate)"
    );

    let done = repo.read_done()?;
    assert_eq!(
        done.tasks.len(),
        1,
        "After run: task should be in done.json"
    );
    let done_task = find_task(&done.tasks, task_id).expect("task should be in done.json");
    assert_eq!(
        done_task.status,
        TaskStatus::Done,
        "After run: status should be Done"
    );
    assert!(
        done_task.started_at.is_some(),
        "After run: started_at should be set"
    );
    assert!(
        done_task.completed_at.is_some(),
        "After run: completed_at should be set"
    );

    Ok(())
}
