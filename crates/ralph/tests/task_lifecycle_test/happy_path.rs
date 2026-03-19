//! Purpose: happy-path lifecycle integration coverage for task transitions.
//!
//! Responsibilities:
//! - Verify the complete create → ready → start → done flow.
//! - Verify the create → ready → start → reject flow.
//!
//! Scope:
//! - End-to-end lifecycle success-path scenarios driven through the real CLI.
//!
//! Usage:
//! - Imported by `task_lifecycle_test.rs`; relies on `use super::*;` for shared fixtures and helpers.
//!
//! Invariants/assumptions callers must respect:
//! - Test names, assertions, and notes remain unchanged from the pre-split suite.
//! - Queue mutation happens through CLI commands except for initial fixture seeding.

use super::test_support;
use super::*;

/// Test the complete happy-path lifecycle: create → ready → start → done.
#[test]
fn task_full_lifecycle_build_ready_start_done() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task_id = "RQ-0001";
    let mut task = draft_task(task_id, "Test task for lifecycle");
    task.description = Some("Test description".to_string());
    task.priority = TaskPriority::Medium;
    task.tags = vec!["test".to_string(), "lifecycle".to_string()];
    task.scope = vec!["crates/ralph/tests".to_string()];
    task.evidence = vec!["integration test".to_string()];
    task.plan = vec!["Step 1".to_string(), "Step 2".to_string()];
    task.request = Some("Test request".to_string());
    repo.write_queue(&[task])?;

    let queue = repo.read_queue()?;
    assert_eq!(queue.tasks.len(), 1, "expected 1 task in queue");
    let task = find_task(&queue.tasks, task_id).expect("task should exist");
    assert_eq!(
        task.status,
        TaskStatus::Draft,
        "initial status should be draft"
    );
    assert!(task.started_at.is_none(), "started_at should not be set");
    assert!(
        task.completed_at.is_none(),
        "completed_at should not be set"
    );

    repo.run_ok(&["task", "ready", task_id])?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist after ready");
    assert_eq!(
        task.status,
        TaskStatus::Todo,
        "status should be todo after ready"
    );
    assert!(
        task.started_at.is_none(),
        "started_at should still not be set"
    );

    repo.run_ok(&["task", "start", task_id])?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist after start");
    assert_eq!(
        task.status,
        TaskStatus::Doing,
        "status should be doing after start"
    );
    assert!(
        task.started_at.is_some(),
        "started_at should be set after start"
    );
    assert!(
        task.started_at
            .as_ref()
            .expect("started_at set")
            .contains('T'),
        "started_at should be a valid RFC3339 timestamp"
    );

    repo.run_ok(&["task", "done", task_id, "--note", "Completed successfully"])?;

    let queue = repo.read_queue()?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "task should be removed from queue after done"
    );

    let done = repo.read_done()?;
    assert_eq!(done.tasks.len(), 1, "expected 1 task in done.json");
    let done_task = find_task(&done.tasks, task_id).expect("task should exist in done.json");
    assert_eq!(done_task.status, TaskStatus::Done, "status should be done");
    assert!(
        done_task.completed_at.is_some(),
        "completed_at should be set after done"
    );
    assert!(
        done_task
            .completed_at
            .as_ref()
            .expect("completed_at set")
            .contains('T'),
        "completed_at should be a valid RFC3339 timestamp"
    );
    assert!(
        done_task
            .notes
            .iter()
            .any(|note| note.contains("Completed successfully")),
        "completion note should be added to task notes: {:?}",
        done_task.notes
    );

    Ok(())
}

/// Test the reject path: create → ready → start → reject.
#[test]
fn task_full_lifecycle_with_reject() -> Result<()> {
    let repo = LifecycleRepo::new()?;

    let task_id = "RQ-0002";
    let task = test_support::make_test_task(task_id, "Task to reject", TaskStatus::Todo);
    repo.write_queue(&[task])?;

    let queue = repo.read_queue()?;
    assert_eq!(queue.tasks.len(), 1);
    let task = find_task(&queue.tasks, task_id).expect("task should exist before reject");
    assert_eq!(task.status, TaskStatus::Todo);

    repo.run_ok(&["task", "start", task_id])?;

    let queue = repo.read_queue()?;
    let task = find_task(&queue.tasks, task_id).expect("task should exist after start");
    assert_eq!(task.status, TaskStatus::Doing);

    repo.run_ok(&[
        "task",
        "reject",
        task_id,
        "--note",
        "Won't fix - out of scope",
    ])?;

    let queue = repo.read_queue()?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "task should be removed from queue"
    );

    let done = repo.read_done()?;
    assert_eq!(done.tasks.len(), 1);
    let done_task = find_task(&done.tasks, task_id).expect("task should be in done.json");
    assert_eq!(
        done_task.status,
        TaskStatus::Rejected,
        "status should be rejected"
    );
    assert!(
        done_task.completed_at.is_some(),
        "completed_at should be set after reject"
    );
    assert!(
        done_task
            .notes
            .iter()
            .any(|note| note.contains("Won't fix")),
        "reject note should be preserved: {:?}",
        done_task.notes
    );

    Ok(())
}
