//! Integration tests for `ralph queue archive`.
//!
//! Responsibilities:
//! - Verify terminal tasks (done/rejected) move from queue → done archive.
//! - Verify no-op behavior when there are no terminal tasks.
//! - Verify done archive creation/usage.
//!
//! Not handled here:
//! - Unit testing of archive internals (covered by module/unit tests).
//! - Exhaustive logging format assertions.
//!
//! Invariants/assumptions:
//! - `ralph init --force --non-interactive` creates a usable `.ralph/` structure.
//! - Archive operates on `.ralph/queue.json` and `.ralph/done.json`.

use anyhow::Result;
use ralph::contracts::TaskStatus;

mod test_support;

#[test]
fn queue_archive_moves_terminal_tasks_to_done() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "Todo", TaskStatus::Todo);
    let mut t2 = test_support::make_test_task("RQ-0002", "Done", TaskStatus::Done);
    let mut t3 = test_support::make_test_task("RQ-0003", "Rejected", TaskStatus::Rejected);
    let t4 = test_support::make_test_task("RQ-0004", "Doing", TaskStatus::Doing);

    // Ensure completed_at is present for terminal tasks.
    t2.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    t3.completed_at = Some("2026-01-21T00:00:00Z".to_string());

    test_support::write_queue(
        dir.path(),
        &[t1.clone(), t2.clone(), t3.clone(), t4.clone()],
    )?;
    test_support::write_done(dir.path(), &[])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["queue", "archive"]);
    anyhow::ensure!(status.success(), "archive failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let done = test_support::read_done(dir.path())?;

    let queue_ids: Vec<_> = queue.tasks.iter().map(|t| t.id.as_str()).collect();
    let done_ids: Vec<_> = done.tasks.iter().map(|t| t.id.as_str()).collect();

    anyhow::ensure!(
        queue_ids == vec!["RQ-0001", "RQ-0004"],
        "unexpected queue: {queue_ids:?}"
    );
    anyhow::ensure!(
        done_ids.contains(&"RQ-0002") && done_ids.contains(&"RQ-0003"),
        "unexpected done: {done_ids:?}"
    );

    Ok(())
}

#[test]
fn queue_archive_is_noop_when_no_terminal_tasks() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "Todo", TaskStatus::Todo);
    let t2 = test_support::make_test_task("RQ-0002", "Doing", TaskStatus::Doing);

    test_support::write_queue(dir.path(), &[t1, t2])?;
    test_support::write_done(dir.path(), &[])?;

    let before_queue = std::fs::read_to_string(dir.path().join(".ralph/queue.json"))?;
    let before_done = std::fs::read_to_string(dir.path().join(".ralph/done.json"))?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["queue", "archive"]);
    anyhow::ensure!(status.success(), "archive failed\nstderr:\n{stderr}");

    let after_queue = std::fs::read_to_string(dir.path().join(".ralph/queue.json"))?;
    let after_done = std::fs::read_to_string(dir.path().join(".ralph/done.json"))?;

    anyhow::ensure!(before_queue == after_queue, "queue changed on noop archive");
    anyhow::ensure!(before_done == after_done, "done changed on noop archive");

    Ok(())
}

#[test]
fn queue_archive_appends_to_existing_done_file() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create an existing done task
    let mut existing_done =
        test_support::make_test_task("RQ-0100", "Already archived", TaskStatus::Done);
    existing_done.completed_at = Some("2026-01-10T00:00:00Z".to_string());
    test_support::write_done(dir.path(), &[existing_done])?;

    // Create a task in queue that will be archived
    let mut t1 = test_support::make_test_task("RQ-0001", "Done task", TaskStatus::Done);
    t1.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    test_support::write_queue(dir.path(), &[t1])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["queue", "archive"]);
    anyhow::ensure!(status.success(), "archive failed\nstderr:\n{stderr}");

    let done = test_support::read_done(dir.path())?;
    anyhow::ensure!(
        done.tasks.iter().any(|t| t.id == "RQ-0001"),
        "archived task should be in done.json"
    );
    anyhow::ensure!(
        done.tasks.iter().any(|t| t.id == "RQ-0100"),
        "existing done task should still be in done.json"
    );

    Ok(())
}
