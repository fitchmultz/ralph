//! Integration tests for `ralph undo` command.
//!
//! Responsibilities:
//! - Verify snapshot creation on queue mutation operations.
//! - Verify `ralph undo --list` output format and content.
//! - Verify `ralph undo` restores queue.json and done.json atomically.
//! - Verify `ralph undo --dry-run` previews without modifying files.
//! - Verify `ralph undo --id <id>` restores a specific snapshot.
//! - Verify snapshot deletion after successful restore.
//!
//! Not handled here:
//! - Unit tests for core undo logic (see crates/ralph/src/undo.rs).
//! - Retention limit enforcement MAX_UNDO_SNAPSHOTS (covered by unit tests, slow for integration).
//!
//! Invariants/assumptions:
//! - Tests run in isolated temp directories outside the repo.
//! - Each test creates its own git repo and ralph project via test_support helpers.
//! - Snapshot IDs are extracted from `ralph undo --list` output.

use anyhow::Result;
use ralph::contracts::TaskStatus;

mod test_support;

/// Test that `ralph undo --list` shows a helpful message when no snapshots exist.
#[test]
fn undo_list_empty_shows_helpful_message() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create a task so the queue is not empty
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;

    // Run undo --list without any mutations
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(
        status.success(),
        "undo --list should succeed even with no snapshots\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stdout.contains("No undo snapshots available"),
        "expected 'No undo snapshots available' message, got:\n{stdout}"
    );

    // Verify the helpful message mentions mutation operations
    anyhow::ensure!(
        stdout.contains("ralph task done")
            || stdout.contains("ralph queue")
            || stdout.contains("mutation"),
        "expected helpful message about mutation operations, got:\n{stdout}"
    );

    Ok(())
}

/// Test that `ralph task done` creates a snapshot that appears in `--list` output.
#[test]
fn undo_list_shows_snapshots_after_task_done() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create task in queue
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Complete the task (creates snapshot)
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "done", "RQ-0001"]);
    anyhow::ensure!(status.success(), "task done failed\nstderr:\n{stderr}");

    // Check that undo --list shows the snapshot
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("Available undo snapshots"),
        "expected 'Available undo snapshots' header, got:\n{stdout}"
    );

    // The snapshot should contain operation description
    anyhow::ensure!(
        stdout.contains("complete_task") || stdout.contains("RQ-0001"),
        "expected operation description in output, got:\n{stdout}"
    );

    Ok(())
}

/// Test that `ralph undo` atomically restores both queue.json and done.json.
#[test]
fn undo_restores_queue_after_task_done() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create initial state
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Record initial state
    let initial_queue = test_support::read_queue(dir.path())?;
    let initial_done = test_support::read_done(dir.path())?;
    anyhow::ensure!(
        initial_queue.tasks.len() == 1,
        "expected 1 task in queue initially"
    );
    anyhow::ensure!(
        initial_done.tasks.is_empty(),
        "expected empty done initially"
    );

    // Complete the task (moves to done.json, creates snapshot)
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "done", "RQ-0001"]);
    anyhow::ensure!(status.success(), "task done failed\nstderr:\n{stderr}");

    // Verify state after done
    let queue_after_done = test_support::read_queue(dir.path())?;
    let done_after_done = test_support::read_done(dir.path())?;
    anyhow::ensure!(
        queue_after_done.tasks.is_empty(),
        "expected empty queue after done"
    );
    anyhow::ensure!(
        done_after_done.tasks.len() == 1,
        "expected 1 task in done after done"
    );
    anyhow::ensure!(
        done_after_done.tasks[0].id == "RQ-0001",
        "expected RQ-0001 in done.json"
    );

    // Undo the operation
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo"]);
    anyhow::ensure!(status.success(), "undo failed\nstderr:\n{stderr}");

    // Verify undo output
    anyhow::ensure!(
        stdout.contains("Restored from snapshot"),
        "expected 'Restored from snapshot' in output, got:\n{stdout}"
    );

    // Verify queue is restored to initial state
    let restored_queue = test_support::read_queue(dir.path())?;
    let restored_done = test_support::read_done(dir.path())?;

    anyhow::ensure!(
        restored_queue.tasks.len() == 1,
        "expected 1 task in queue after restore, got {} tasks",
        restored_queue.tasks.len()
    );
    anyhow::ensure!(
        restored_queue.tasks[0].id == "RQ-0001",
        "expected RQ-0001 in queue after restore, got {:?}",
        restored_queue.tasks[0].id
    );
    anyhow::ensure!(
        restored_queue.tasks[0].status == TaskStatus::Todo,
        "expected status Todo after restore, got {:?}",
        restored_queue.tasks[0].status
    );

    // Verify done.json is restored to initial state (empty)
    anyhow::ensure!(
        restored_done.tasks.is_empty(),
        "expected empty done.json after restore, got {} tasks",
        restored_done.tasks.len()
    );

    Ok(())
}

/// Test that `ralph undo --dry-run` previews without modifying files.
#[test]
fn undo_dry_run_does_not_modify_files() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create initial state
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Complete the task (creates snapshot)
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "done", "RQ-0001"]);
    anyhow::ensure!(status.success(), "task done failed\nstderr:\n{stderr}");

    // Record state after done (before dry-run undo)
    let queue_before = test_support::read_queue(dir.path())?;
    let done_before = test_support::read_done(dir.path())?;

    // Run dry-run undo
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--dry-run"]);
    anyhow::ensure!(status.success(), "undo --dry-run failed\nstderr:\n{stderr}");

    // Verify output mentions dry run
    anyhow::ensure!(
        stdout.contains("Dry run") || stdout.contains("dry run"),
        "expected 'Dry run' in output, got:\n{stdout}"
    );

    // Verify files are unchanged
    let queue_after = test_support::read_queue(dir.path())?;
    let done_after = test_support::read_done(dir.path())?;

    anyhow::ensure!(
        queue_before.tasks.len() == queue_after.tasks.len(),
        "queue.json was modified during dry run"
    );
    anyhow::ensure!(
        done_before.tasks.len() == done_after.tasks.len(),
        "done.json was modified during dry run"
    );

    // Verify queue is still empty and done still has the task
    anyhow::ensure!(
        queue_after.tasks.is_empty(),
        "queue.json should still be empty after dry run"
    );
    anyhow::ensure!(
        done_after.tasks.len() == 1,
        "done.json should still have the task after dry run"
    );

    Ok(())
}

/// Test that `ralph undo --id <id>` restores the specified snapshot.
///
/// This test verifies that:
/// 1. First snapshot is created before RQ-0001 is marked done (capturing initial state)
/// 2. Second snapshot is created before RQ-0002 is marked done (capturing state with RQ-0001 done)
/// 3. Restoring from the second snapshot brings back the state where RQ-0001 is done and RQ-0002 is in queue
#[test]
fn undo_with_specific_id_restores_correct_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create two tasks
    let task1 = test_support::make_test_task("RQ-0001", "First task", TaskStatus::Todo);
    let task2 = test_support::make_test_task("RQ-0002", "Second task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task1, task2])?;
    test_support::write_done(dir.path(), &[])?;

    // Complete first task (creates first snapshot of initial state)
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "done", "RQ-0001"]);
    anyhow::ensure!(
        status.success(),
        "task done RQ-0001 failed\nstderr:\n{stderr}"
    );

    // Complete second task (creates second snapshot of state with RQ-0001 done)
    let (status, _, stderr) = test_support::run_in_dir(dir.path(), &["task", "done", "RQ-0002"]);
    anyhow::ensure!(
        status.success(),
        "task done RQ-0002 failed\nstderr:\n{stderr}"
    );

    // Verify state after both tasks done (queue should be empty, done has both)
    let queue_after_both = test_support::read_queue(dir.path())?;
    let done_after_both = test_support::read_done(dir.path())?;
    anyhow::ensure!(
        queue_after_both.tasks.is_empty(),
        "expected empty queue after both tasks done"
    );
    anyhow::ensure!(
        done_after_both.tasks.len() == 2,
        "expected 2 tasks in done after both done"
    );

    // Get both snapshot IDs from undo --list (newest first)
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    // Extract snapshot IDs (newest first, so second snapshot is first in list)
    let snapshot_ids: Vec<String> = stdout
        .lines()
        .filter(|line| line.contains("ID:"))
        .filter_map(|line| line.split("ID:").nth(1))
        .map(|s| s.trim().to_string())
        .collect();

    anyhow::ensure!(
        snapshot_ids.len() == 2,
        "expected 2 snapshots, found {}\noutput:\n{stdout}",
        snapshot_ids.len()
    );

    // The second snapshot (index 0, newest) captures state after RQ-0001 was done
    // (i.e., queue had RQ-0002, done had RQ-0001)
    let second_snapshot_id = &snapshot_ids[0];

    // Restore using the second snapshot ID
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["undo", "--id", second_snapshot_id]);
    anyhow::ensure!(
        status.success(),
        "undo --id {second_snapshot_id} failed\nstderr:\n{stderr}"
    );

    // Verify we restored to the second snapshot state:
    // - RQ-0002 should be back in queue (it was in queue when second snapshot was taken)
    // - RQ-0001 should be in done (it was moved there before second snapshot)
    let restored_queue = test_support::read_queue(dir.path())?;
    let restored_done = test_support::read_done(dir.path())?;

    anyhow::ensure!(
        restored_queue.tasks.len() == 1,
        "expected 1 task in queue after restoring second snapshot, got {} tasks",
        restored_queue.tasks.len()
    );
    anyhow::ensure!(
        restored_queue.tasks[0].id == "RQ-0002",
        "expected RQ-0002 in queue after restore, got {:?}",
        restored_queue.tasks[0].id
    );

    anyhow::ensure!(
        restored_done.tasks.len() == 1,
        "expected 1 task in done after restoring second snapshot, got {} tasks",
        restored_done.tasks.len()
    );
    anyhow::ensure!(
        restored_done.tasks[0].id == "RQ-0001",
        "expected RQ-0001 in done after restore, got {:?}",
        restored_done.tasks[0].id
    );

    Ok(())
}

/// Test that the snapshot file is deleted after successful restore.
#[test]
fn undo_removes_used_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create task
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Complete the task (creates snapshot)
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "done", "RQ-0001"]);
    anyhow::ensure!(status.success(), "task done failed\nstderr:\n{stderr}");

    // Verify snapshot exists
    let undo_dir = dir.path().join(".ralph/cache/undo");
    anyhow::ensure!(
        undo_dir.exists(),
        "undo directory should exist after mutation"
    );

    let snapshots_before: Vec<_> = std::fs::read_dir(&undo_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect();
    anyhow::ensure!(
        !snapshots_before.is_empty(),
        "expected at least one snapshot after mutation"
    );

    // Undo the operation
    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo"]);
    anyhow::ensure!(status.success(), "undo failed\nstderr:\n{stderr}");

    // Verify snapshot is removed
    let snapshots_after: Vec<_> = std::fs::read_dir(&undo_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect();

    anyhow::ensure!(
        snapshots_after.is_empty(),
        "expected no snapshots after undo, found {}",
        snapshots_after.len()
    );

    Ok(())
}

/// Test that `ralph undo` fails with a clear error when no snapshots exist.
#[test]
fn undo_no_snapshots_error() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create a task but don't perform any mutation that creates snapshots
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;

    // Try to undo without any snapshots
    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo"]);

    // Should fail (non-zero exit status)
    anyhow::ensure!(
        !status.success(),
        "undo should fail when no snapshots exist"
    );

    // Should have a helpful error message
    anyhow::ensure!(
        stderr.contains("No undo snapshots available")
            || stderr.contains("no undo snapshots")
            || stderr.contains("No snapshots"),
        "expected 'No undo snapshots available' error, got stderr:\n{stderr}"
    );

    Ok(())
}

/// Test that `ralph task reject` also creates snapshots.
#[test]
fn undo_creates_snapshot_on_task_reject() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create task
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Reject the task (should create snapshot)
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "reject", "RQ-0001"]);
    anyhow::ensure!(status.success(), "task reject failed\nstderr:\n{stderr}");

    // Verify snapshot exists
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("Available undo snapshots"),
        "expected snapshot after task reject, got:\n{stdout}"
    );

    // Verify we can undo the reject
    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo"]);
    anyhow::ensure!(
        status.success(),
        "undo after reject failed\nstderr:\n{stderr}"
    );

    // Verify task is back in queue with todo status
    let queue = test_support::read_queue(dir.path())?;
    anyhow::ensure!(
        queue.tasks.len() == 1,
        "expected 1 task in queue after undo"
    );
    anyhow::ensure!(
        queue.tasks[0].status == TaskStatus::Todo,
        "expected status Todo after undo, got {:?}",
        queue.tasks[0].status
    );

    Ok(())
}

/// Test that `ralph queue archive` creates a snapshot.
#[test]
fn undo_creates_snapshot_on_queue_archive() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create done task in queue (ready for archive)
    let mut task = test_support::make_test_task("RQ-0001", "Done task", TaskStatus::Done);
    task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Archive the queue (should create snapshot)
    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["queue", "archive"]);
    anyhow::ensure!(status.success(), "queue archive failed\nstderr:\n{stderr}");

    // Verify snapshot exists
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("Available undo snapshots"),
        "expected snapshot after queue archive, got:\n{stdout}"
    );

    // Verify we can undo the archive
    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo"]);
    anyhow::ensure!(
        status.success(),
        "undo after archive failed\nstderr:\n{stderr}"
    );

    // Verify task is back in queue (not in done.json)
    let queue = test_support::read_queue(dir.path())?;
    let done = test_support::read_done(dir.path())?;
    anyhow::ensure!(
        queue.tasks.len() == 1,
        "expected 1 task in queue after undo archive"
    );
    anyhow::ensure!(
        done.tasks.is_empty(),
        "expected empty done.json after undo archive"
    );

    Ok(())
}
