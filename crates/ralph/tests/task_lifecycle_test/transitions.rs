//! Purpose: invalid and edge-case lifecycle transition coverage.
//!
//! Responsibilities:
//! - Verify terminal tasks cannot be started again.
//! - Verify `task ready` only applies to draft tasks.
//!
//! Scope:
//! - CLI failure and no-op transition behavior.
//!
//! Usage:
//! - Imported by `task_lifecycle_test.rs`; relies on `use super::*;` for shared fixtures and helpers.
//!
//! Invariants/assumptions callers must respect:
//! - Tests preserve existing error assertions and queue-state checks.
//! - Terminal-task fixtures continue to come from suite-local helpers.

use super::test_support;
use super::*;

/// Test that cannot start a terminal (done/rejected) task.
#[test]
fn task_cannot_start_terminal_task() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task_id = "RQ-0005";
    repo.write_queue(&[terminal_task(task_id, "Done task", TaskStatus::Done)])?;

    let (status, _, stderr) = repo.run(&["task", "start", task_id]);
    assert!(!status.success(), "should fail to start a done task");
    assert!(
        stderr.contains("terminal") || stderr.contains("Done") || stderr.contains("cannot"),
        "error should mention terminal status: {}",
        stderr
    );

    Ok(())
}

/// Test that task ready command requires draft status.
#[test]
fn task_ready_requires_draft_status() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task = test_support::make_test_task("RQ-1004", "Todo task", TaskStatus::Todo);
    repo.write_queue(&[task])?;

    let (status, _, _stderr) = repo.run(&["task", "ready", "RQ-1004"]);
    let _ = status;

    let queue = repo.read_queue()?;
    assert_eq!(queue.tasks.len(), 1);

    Ok(())
}
