//! Purpose: multi-task lifecycle independence coverage.
//!
//! Responsibilities:
//! - Verify multiple tasks can progress through separate lifecycle outcomes without cross-contamination.
//!
//! Scope:
//! - Queue and done-state assertions for concurrent fixture tasks in one repo.
//!
//! Usage:
//! - Imported by `task_lifecycle_test.rs`; relies on `use super::*;` for shared fixtures and helpers.
//!
//! Invariants/assumptions callers must respect:
//! - Only the targeted task should change during each CLI operation.
//! - Assertions remain unchanged from the pre-split suite.

use super::test_support;
use super::*;

/// Test multiple tasks lifecycle independently.
#[test]
fn task_multiple_independent_lifecycles() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task1 = test_support::make_test_task("RQ-1001", "First task", TaskStatus::Todo);
    let task2 = test_support::make_test_task("RQ-1002", "Second task", TaskStatus::Todo);
    let task3 = test_support::make_test_task("RQ-1003", "Third task", TaskStatus::Todo);
    repo.write_queue(&[task1, task2, task3])?;

    repo.run_ok(&["task", "start", "RQ-1001"])?;

    let queue = repo.read_queue()?;
    let t1 = find_task(&queue.tasks, "RQ-1001").expect("task 1 should exist");
    let t2 = find_task(&queue.tasks, "RQ-1002").expect("task 2 should exist");
    let t3 = find_task(&queue.tasks, "RQ-1003").expect("task 3 should exist");
    assert_eq!(t1.status, TaskStatus::Doing);
    assert_eq!(t2.status, TaskStatus::Todo);
    assert_eq!(t3.status, TaskStatus::Todo);

    repo.run_ok(&["task", "done", "RQ-1001"])?;

    let queue = repo.read_queue()?;
    assert_eq!(queue.tasks.len(), 2);
    assert!(find_task(&queue.tasks, "RQ-1001").is_none());

    repo.run_ok(&["task", "reject", "RQ-1002"])?;

    let queue = repo.read_queue()?;
    assert_eq!(queue.tasks.len(), 1);
    assert!(find_task(&queue.tasks, "RQ-1002").is_none());

    let done = repo.read_done()?;
    assert_eq!(done.tasks.len(), 2);
    let done_t1 = find_task(&done.tasks, "RQ-1001").expect("done task 1 should exist");
    let done_t2 = find_task(&done.tasks, "RQ-1002").expect("done task 2 should exist");
    assert_eq!(done_t1.status, TaskStatus::Done);
    assert_eq!(done_t2.status, TaskStatus::Rejected);

    let t3 = find_task(&queue.tasks, "RQ-1003").expect("task 3 should remain in queue");
    assert_eq!(t3.status, TaskStatus::Todo);

    Ok(())
}
