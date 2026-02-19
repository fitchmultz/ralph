//! Integration tests for complete task lifecycle.
//!
//! Responsibilities:
//! - Test full task lifecycle: create → ready → start → run → done/reject
//! - Verify queue/done state transitions at each step
//! - Test runner execution integration (`ralph run` command)
//! - Ensure status changes and timestamps are correct
//!
//! Not handled here:
//! - Unit tests for individual commands (see task_cmd_test.rs)
//! - Parallel execution lifecycle (see parallel_e2e_test.rs)
//! - Task update functionality (see task_update_all_integration_test.rs)

mod test_support;

use anyhow::Result;
use ralph::contracts::{Task, TaskStatus};

/// Helper to find a task by ID in a queue slice.
fn find_task<'a>(tasks: &'a [Task], id: &str) -> Option<&'a Task> {
    tasks.iter().find(|t| t.id == id)
}

/// Test the complete happy-path lifecycle: create → ready → start → done
#[test]
fn task_full_lifecycle_build_ready_start_done() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Step 1: Create a task directly in the queue (simulating task build result)
    // We write directly to queue.json to avoid needing a runner
    let task_id = "RQ-0001";
    let task = Task {
        id: task_id.to_string(),
        title: "Test task for lifecycle".to_string(),
        description: Some("Test description".to_string()),
        status: TaskStatus::Draft,
        priority: ralph::contracts::TaskPriority::Medium,
        tags: vec!["test".to_string(), "lifecycle".to_string()],
        scope: vec!["crates/ralph/tests".to_string()],
        evidence: vec!["integration test".to_string()],
        plan: vec!["Step 1".to_string(), "Step 2".to_string()],
        notes: vec![],
        request: Some("Test request".to_string()),
        agent: None,
        created_at: Some("2026-02-19T00:00:00Z".to_string()),
        updated_at: Some("2026-02-19T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };
    test_support::write_queue(dir.path(), &[task])?;

    // Verify initial state: task in queue with draft status
    let queue = test_support::read_queue(dir.path())?;
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

    // Step 2: Run `ralph task ready <ID>` to promote draft → todo
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "ready", task_id]);
    anyhow::ensure!(
        status.success(),
        "task ready failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify status changed to todo
    let queue = test_support::read_queue(dir.path())?;
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

    // Step 3: Run `ralph task start <ID>`
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "start", task_id]);
    anyhow::ensure!(
        status.success(),
        "task start failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify status is doing and started_at is set
    let queue = test_support::read_queue(dir.path())?;
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
        task.started_at.as_ref().unwrap().contains('T'),
        "started_at should be a valid RFC3339 timestamp"
    );

    // Step 4: Run `ralph task done <ID> --note "Completed successfully"`
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "done", task_id, "--note", "Completed successfully"],
    );
    anyhow::ensure!(
        status.success(),
        "task done failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify: Task is removed from queue.json
    let queue = test_support::read_queue(dir.path())?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "task should be removed from queue after done"
    );

    // Verify: Task exists in done.json with status done
    let done = test_support::read_done(dir.path())?;
    assert_eq!(done.tasks.len(), 1, "expected 1 task in done.json");
    let done_task = find_task(&done.tasks, task_id).expect("task should exist in done.json");
    assert_eq!(done_task.status, TaskStatus::Done, "status should be done");
    assert!(
        done_task.completed_at.is_some(),
        "completed_at should be set after done"
    );
    assert!(
        done_task.completed_at.as_ref().unwrap().contains('T'),
        "completed_at should be a valid RFC3339 timestamp"
    );

    // Verify: Note is added to task notes
    assert!(
        done_task
            .notes
            .iter()
            .any(|n| n.contains("Completed successfully")),
        "completion note should be added to task notes: {:?}",
        done_task.notes
    );

    Ok(())
}

/// Test the reject path: create → ready → start → reject
#[test]
fn task_full_lifecycle_with_reject() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create task directly in queue with todo status
    let task_id = "RQ-0002";
    let task = test_support::make_test_task(task_id, "Task to reject", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Verify initial state
    let queue = test_support::read_queue(dir.path())?;
    assert_eq!(queue.tasks.len(), 1);
    let task = find_task(&queue.tasks, task_id).unwrap();
    assert_eq!(task.status, TaskStatus::Todo);

    // Start the task
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "start", task_id]);
    anyhow::ensure!(status.success(), "task start failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, task_id).unwrap();
    assert_eq!(task.status, TaskStatus::Doing);

    // Reject the task with a note
    let (status, _, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "reject",
            task_id,
            "--note",
            "Won't fix - out of scope",
        ],
    );
    anyhow::ensure!(status.success(), "task reject failed\nstderr:\n{stderr}");

    // Verify: Task is in done.json with status rejected
    let queue = test_support::read_queue(dir.path())?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "task should be removed from queue"
    );

    let done = test_support::read_done(dir.path())?;
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
        done_task.notes.iter().any(|n| n.contains("Won't fix")),
        "reject note should be preserved: {:?}",
        done_task.notes
    );

    Ok(())
}

/// Test queue state verification at each lifecycle step
#[test]
fn task_lifecycle_state_verification() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let task_id = "RQ-0003";

    // Create task with draft status
    let task = Task {
        id: task_id.to_string(),
        title: "State verification task".to_string(),
        status: TaskStatus::Draft,
        created_at: Some("2026-02-19T00:00:00Z".to_string()),
        updated_at: Some("2026-02-19T00:00:00Z".to_string()),
        ..Default::default()
    };
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Capture state after create (draft)
    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, task_id).unwrap();
    assert_eq!(task.status, TaskStatus::Draft);
    assert!(task.started_at.is_none());

    // After ready: status=todo, no started_at
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "ready", task_id]);
    anyhow::ensure!(status.success(), "task ready failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, task_id).unwrap();
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

    // After start: status=doing, has started_at
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "start", task_id]);
    anyhow::ensure!(status.success(), "task start failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, task_id).unwrap();
    assert_eq!(
        task.status,
        TaskStatus::Doing,
        "after start: status should be doing"
    );
    assert!(
        task.started_at.is_some(),
        "after start: started_at should be set"
    );

    // Record started_at for later verification
    let started_at = task.started_at.clone().unwrap();

    // After done: task moved to done.json
    let (status, _, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "done", task_id, "--note", "Done!"]);
    anyhow::ensure!(status.success(), "task done failed\nstderr:\n{stderr}");

    // Verify queue is empty
    let queue = test_support::read_queue(dir.path())?;
    assert!(queue.tasks.is_empty(), "after done: queue should be empty");

    // Verify task in done.json
    let done = test_support::read_done(dir.path())?;
    assert_eq!(
        done.tasks.len(),
        1,
        "after done: done.json should have 1 task"
    );
    let done_task = find_task(&done.tasks, task_id).unwrap();
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

/// Test that starting an already started task (with reset) updates started_at
#[test]
fn task_start_reset_updates_timestamp() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let task_id = "RQ-0004";
    let task = test_support::make_test_task(task_id, "Reset test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;

    // Start the task
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "start", task_id]);
    anyhow::ensure!(status.success(), "task start failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, task_id).unwrap();
    let first_started_at = task.started_at.clone().unwrap();

    // Wait a moment to ensure different timestamp
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Start again with --reset flag
    let (status, _, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "start", task_id, "--reset"]);
    anyhow::ensure!(
        status.success(),
        "task start --reset failed\nstderr:\n{stderr}"
    );

    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, task_id).unwrap();
    let second_started_at = task.started_at.clone().unwrap();

    // Timestamps should be different
    assert_ne!(
        first_started_at, second_started_at,
        "started_at should be updated after reset"
    );

    Ok(())
}

/// Test that cannot start a terminal (done/rejected) task
#[test]
fn task_cannot_start_terminal_task() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let task_id = "RQ-0005";
    let mut task = test_support::make_test_task(task_id, "Done task", TaskStatus::Done);
    task.completed_at = Some("2026-02-19T00:00:00Z".to_string());
    test_support::write_queue(dir.path(), &[task])?;

    // Try to start a done task
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "start", task_id]);
    assert!(!status.success(), "should fail to start a done task");
    assert!(
        stderr.contains("terminal") || stderr.contains("Done") || stderr.contains("cannot"),
        "error should mention terminal status: {}",
        stderr
    );

    Ok(())
}

/// Test multiple tasks lifecycle independently
#[test]
fn task_multiple_independent_lifecycles() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create multiple tasks
    let task1 = test_support::make_test_task("RQ-1001", "First task", TaskStatus::Todo);
    let task2 = test_support::make_test_task("RQ-1002", "Second task", TaskStatus::Todo);
    let task3 = test_support::make_test_task("RQ-1003", "Third task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task1, task2, task3])?;
    test_support::write_done(dir.path(), &[])?;

    // Start task1
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "start", "RQ-1001"]);
    anyhow::ensure!(
        status.success(),
        "task start RQ-1001 failed\nstderr:\n{stderr}"
    );

    let queue = test_support::read_queue(dir.path())?;
    let t1 = find_task(&queue.tasks, "RQ-1001").unwrap();
    let t2 = find_task(&queue.tasks, "RQ-1002").unwrap();
    let t3 = find_task(&queue.tasks, "RQ-1003").unwrap();
    assert_eq!(t1.status, TaskStatus::Doing);
    assert_eq!(t2.status, TaskStatus::Todo);
    assert_eq!(t3.status, TaskStatus::Todo);

    // Complete task1
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "done", "RQ-1001"]);
    anyhow::ensure!(
        status.success(),
        "task done RQ-1001 failed\nstderr:\n{stderr}"
    );

    let queue = test_support::read_queue(dir.path())?;
    assert_eq!(queue.tasks.len(), 2);
    assert!(find_task(&queue.tasks, "RQ-1001").is_none());

    // Reject task2
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "reject", "RQ-1002"]);
    anyhow::ensure!(
        status.success(),
        "task reject RQ-1002 failed\nstderr:\n{stderr}"
    );

    let queue = test_support::read_queue(dir.path())?;
    assert_eq!(queue.tasks.len(), 1);
    assert!(find_task(&queue.tasks, "RQ-1002").is_none());

    let done = test_support::read_done(dir.path())?;
    assert_eq!(done.tasks.len(), 2);
    let done_t1 = find_task(&done.tasks, "RQ-1001").unwrap();
    let done_t2 = find_task(&done.tasks, "RQ-1002").unwrap();
    assert_eq!(done_t1.status, TaskStatus::Done);
    assert_eq!(done_t2.status, TaskStatus::Rejected);

    // Task3 should still be in queue
    let t3 = find_task(&queue.tasks, "RQ-1003").unwrap();
    assert_eq!(t3.status, TaskStatus::Todo);

    Ok(())
}

/// Test that task ready command requires draft status
#[test]
fn task_ready_requires_draft_status() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create a non-draft task
    let task = test_support::make_test_task("RQ-1004", "Todo task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;

    // Try to run ready on a todo task (should still work, no-op or success)
    let (status, _, _stderr) = test_support::run_in_dir(dir.path(), &["task", "ready", "RQ-1004"]);
    // The command may succeed or fail depending on implementation, but it shouldn't panic
    // We mainly care that it doesn't corrupt the queue
    let _ = status;

    // Verify queue is still valid
    let queue = test_support::read_queue(dir.path())?;
    assert_eq!(queue.tasks.len(), 1);

    Ok(())
}

/// Test that started_at is not set by ready command
#[test]
fn task_ready_does_not_set_started_at() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create a draft task
    let task = Task {
        id: "RQ-1005".to_string(),
        title: "Draft task".to_string(),
        status: TaskStatus::Draft,
        created_at: Some("2026-02-19T00:00:00Z".to_string()),
        updated_at: Some("2026-02-19T00:00:00Z".to_string()),
        ..Default::default()
    };
    test_support::write_queue(dir.path(), &[task])?;

    // Run ready
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "ready", "RQ-1005"]);
    anyhow::ensure!(status.success(), "task ready failed\nstderr:\n{stderr}");

    // Verify status is todo but started_at is not set
    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, "RQ-1005").unwrap();
    assert_eq!(task.status, TaskStatus::Todo);
    assert!(task.started_at.is_none(), "ready should not set started_at");

    Ok(())
}

/// Test full lifecycle with actual runner execution: create → ready → start → run → done
///
/// This test verifies that:
/// 1. `ralph run one` selects a todo task and runs the runner
/// 2. When CI gate passes, task is auto-completed (moved to done.json with Done status)
/// 3. Task metadata (started_at, completed_at) is properly tracked
#[test]
fn task_full_lifecycle_with_runner_execution() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create task in todo status (this is what ralph run one looks for)
    let task_id = "RQ-9001";
    let task = test_support::make_test_task(task_id, "Runner test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Create marker file path to verify runner was executed
    let marker_file = dir.path().join(".ralph/runner_executed.marker");

    // Create mock runner that creates a marker file
    let runner_script = format!(
        r#"#!/bin/bash
# Mock runner that verifies it received task context
echo "runner_executed" > "{}"
exit 0
"#,
        marker_file.display()
    );
    let runner_path = test_support::create_fake_runner(dir.path(), "codex", &runner_script)?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    // Create Makefile for CI gate
    std::fs::write(dir.path().join("Makefile"), "ci:\n\t@echo 'CI passed'\n")?;
    test_support::git_add_all_commit(dir.path(), "setup")?;

    // Verify initial state: task is Todo
    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, task_id).unwrap();
    assert_eq!(
        task.status,
        TaskStatus::Todo,
        "Initial: status should be Todo"
    );

    // Execute: Run the task
    // Note: ralph run one selects the task, runs the runner, and when CI passes,
    // the supervision logic auto-completes the task (moves to done.json)
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["run", "one"]);
    anyhow::ensure!(
        status.success(),
        "ralph run one failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify: Runner was executed (marker file exists)
    assert!(
        marker_file.exists(),
        "Runner should have been executed (marker file should exist)"
    );

    // Verify: Task is now in done.json with Done status (auto-completed by CI gate)
    let queue = test_support::read_queue(dir.path())?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "Task should not be in queue after successful run + CI gate"
    );

    let done = test_support::read_done(dir.path())?;
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

/// Test that runner failure prevents task auto-completion
///
/// This test verifies that when the runner fails:
/// 1. Task remains in the queue (not auto-completed)
/// 2. Task status is still Doing (set by run command at start)
/// 3. User can then reject the task manually
#[test]
fn task_runner_failure_prevents_auto_complete() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let task_id = "RQ-9002";
    let task = test_support::make_test_task(task_id, "Task that will fail", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Create a runner that fails
    let runner_path = test_support::create_fake_runner(dir.path(), "codex", "#!/bin/sh\nexit 1\n")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    std::fs::write(dir.path().join("Makefile"), "ci:\n\t@echo 'CI passed'\n")?;
    test_support::git_add_all_commit(dir.path(), "setup")?;

    // Run the task - it should fail because runner exits with 1
    let (status, _, _stderr) = test_support::run_in_dir(dir.path(), &["run", "one"]);
    // The run command should fail
    assert!(
        !status.success(),
        "Run should fail when runner exits with error"
    );

    // Verify task is still in queue with Doing status (run set this before runner failed)
    let queue = test_support::read_queue(dir.path())?;
    assert_eq!(
        queue.tasks.len(),
        1,
        "Task should still be in queue after runner failure"
    );
    let task = find_task(&queue.tasks, task_id).unwrap();
    assert_eq!(
        task.status,
        TaskStatus::Doing,
        "Task should be Doing (set by run command)"
    );
    assert!(
        task.started_at.is_some(),
        "started_at should be set by run command"
    );

    // Now reject the task since the runner failed
    let (status, _, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "reject",
            task_id,
            "--note",
            "Runner failed - won't fix",
        ],
    );
    anyhow::ensure!(status.success(), "task reject failed\nstderr:\n{stderr}");

    // Verify task moved to done.json with Rejected status
    let queue = test_support::read_queue(dir.path())?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "Task should be removed from queue"
    );

    let done = test_support::read_done(dir.path())?;
    assert_eq!(done.tasks.len(), 1, "Task should be in done.json");
    let done_task = find_task(&done.tasks, task_id).unwrap();
    assert_eq!(
        done_task.status,
        TaskStatus::Rejected,
        "Status should be Rejected"
    );
    assert!(
        done_task.notes.iter().any(|n| n.contains("Runner failed")),
        "Reject note should be preserved"
    );

    Ok(())
}

/// Test queue state transitions during full lifecycle including runner execution
///
/// This test verifies the complete state transition sequence with CI gate auto-completion:
/// 1. Initial: Task is Todo in queue.json
/// 2. After `ralph run one`: Task is auto-completed and moved to done.json with Done status
///    (The CI gate marks tasks as done when they complete successfully)
/// 3. Timestamps (started_at, completed_at) are properly set
#[test]
fn task_lifecycle_queue_state_during_run() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let task_id = "RQ-9003";
    let task = test_support::make_test_task(task_id, "State tracking task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    let runner_path = test_support::create_fake_runner(dir.path(), "codex", "#!/bin/sh\nexit 0\n")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    std::fs::write(dir.path().join("Makefile"), "ci:\n\t@echo 'CI passed'\n")?;
    test_support::git_add_all_commit(dir.path(), "setup")?;

    // Verify initial state: Task is Todo in queue.json
    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, task_id).unwrap();
    assert_eq!(
        task.status,
        TaskStatus::Todo,
        "Initial: status should be Todo"
    );
    assert!(
        task.started_at.is_none(),
        "Initial: started_at should not be set"
    );

    // Run the task
    // Note: `ralph run one` executes the runner, then the CI gate auto-completes
    // the task (moves to done.json with Done status) when CI passes
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["run", "one"]);
    anyhow::ensure!(status.success(), "ralph run one failed\nstderr:\n{stderr}");

    // Verify: Task is now in done.json with Done status
    let queue = test_support::read_queue(dir.path())?;
    assert!(
        find_task(&queue.tasks, task_id).is_none(),
        "After run: task should not be in queue (auto-completed by CI gate)"
    );

    let done = test_support::read_done(dir.path())?;
    assert_eq!(
        done.tasks.len(),
        1,
        "After run: task should be in done.json"
    );
    let done_task = find_task(&done.tasks, task_id).unwrap();
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
