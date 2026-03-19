//! Purpose: lifecycle state and timestamp verification coverage.
//!
//! Responsibilities:
//! - Verify queue and done state at each lifecycle step.
//! - Confirm `ready` does not set `started_at`.
//! - Confirm `start --reset` updates `started_at`.
//!
//! Scope:
//! - Timestamp and state assertion scenarios for a single task lifecycle.
//!
//! Usage:
//! - Imported by `task_lifecycle_test.rs`; relies on `use super::*;` for shared fixtures and helpers.
//!
//! Invariants/assumptions callers must respect:
//! - Assertions remain unchanged from the original suite.
//! - Tests continue to use real CLI transitions instead of direct post-seed file mutation.

use super::test_support;
use super::*;

/// Test queue state verification at each lifecycle step.
#[test]
fn task_lifecycle_state_verification() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task_id = "RQ-0003";
    repo.write_queue(&[draft_task(task_id, "State verification task")])?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist before ready");
    assert_eq!(task.status, TaskStatus::Draft);
    assert!(task.started_at.is_none());

    repo.run_ok(&["task", "ready", task_id])?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist after ready");
    assert_eq!(
        task.status,
        TaskStatus::Todo,
        "after ready: status should be todo"
    );
    assert!(
        task.started_at.is_none(),
        "after ready: started_at should not be set"
    );
    assert!(
        task.updated_at.is_some(),
        "after ready: updated_at should be set"
    );

    repo.run_ok(&["task", "start", task_id])?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist after start");
    assert_eq!(
        task.status,
        TaskStatus::Doing,
        "after start: status should be doing"
    );
    assert!(
        task.started_at.is_some(),
        "after start: started_at should be set"
    );

    let started_at = task.started_at.clone().expect("started_at should exist");

    repo.run_ok(&["task", "done", task_id, "--note", "Done!"])?;

    let queue = repo.read_queue()?;
    assert!(queue.tasks.is_empty(), "after done: queue should be empty");

    let done = repo.read_done()?;
    assert_eq!(
        done.tasks.len(),
        1,
        "after done: done.json should have 1 task"
    );
    let done_task = find_task(&done.tasks, task_id).expect("task should exist in done.json");
    assert_eq!(
        done_task.status,
        TaskStatus::Done,
        "after done: status should be done"
    );
    assert_eq!(
        done_task.started_at.as_ref(),
        Some(&started_at),
        "after done: started_at should be preserved"
    );
    assert!(
        done_task.completed_at.is_some(),
        "after done: completed_at should be set"
    );

    Ok(())
}

/// Test that starting an already started task (with reset) updates started_at.
#[test]
fn task_start_reset_updates_timestamp() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task_id = "RQ-0004";
    let task = test_support::make_test_task(task_id, "Reset test task", TaskStatus::Todo);
    repo.write_queue(&[task])?;

    repo.run_ok(&["task", "start", task_id])?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist after initial start");
    let first_started_at = task.started_at.clone().expect("started_at should be set");

    let mut second_started_at = first_started_at.clone();
    for _ in 0..8 {
        repo.run_ok(&["task", "start", task_id, "--reset"])?;

        let queue = repo.read_queue()?;
        let task = find_task(&queue.tasks, task_id).expect("task should exist after reset");
        second_started_at = task.started_at.clone().expect("started_at should be set");
        if second_started_at != first_started_at {
            break;
        }
    }

    assert_ne!(
        first_started_at, second_started_at,
        "started_at should be updated after reset"
    );

    Ok(())
}

/// Test that started_at is not set by ready command.
#[test]
fn task_ready_does_not_set_started_at() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    repo.write_queue(&[draft_task("RQ-1005", "Draft task")])?;

    repo.run_ok(&["task", "ready", "RQ-1005"])?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, "RQ-1005").expect("task should exist after ready");
    assert_eq!(task.status, TaskStatus::Todo);
    assert!(task.started_at.is_none(), "ready should not set started_at");

    Ok(())
}
