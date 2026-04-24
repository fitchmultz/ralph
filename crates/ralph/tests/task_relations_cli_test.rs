//! Integration tests for task relationship commands.
//!
//! Purpose:
//! - Integration tests for task relationship commands.
//!
//! Responsibilities:
//! - Verify `task relate` updates the specified relationship field.
//! - Verify `task blocks` appends blocked IDs without duplication.
//! - Verify `task mark-duplicate` sets `duplicates`.
//! - Verify invalid relationship types fail cleanly.
//!
//! Not handled here:
//! - Bidirectional relationship enforcement (only assert current contract).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue mutations persist to `.ralph/queue.json`.
//! - Relationship storage matches `Task` contract fields: `blocks`, `relates_to`, `duplicates`.

use anyhow::Result;
use ralph::contracts::TaskStatus;

mod test_support;

#[test]
fn task_relate_blocks_updates_queue() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "A", TaskStatus::Todo);
    let t2 = test_support::make_test_task("RQ-0002", "B", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1, t2])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "relate", "RQ-0001", "blocks", "RQ-0002"],
    );
    anyhow::ensure!(status.success(), "task relate failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let a = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task A");
    anyhow::ensure!(
        a.blocks.contains(&"RQ-0002".to_string()),
        "blocks not updated"
    );

    Ok(())
}

#[test]
fn task_relate_relates_to_updates_queue() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "A", TaskStatus::Todo);
    let t2 = test_support::make_test_task("RQ-0002", "B", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1, t2])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "relate", "RQ-0001", "relates_to", "RQ-0002"],
    );
    anyhow::ensure!(status.success(), "task relate failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let a = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task A");
    anyhow::ensure!(
        a.relates_to.contains(&"RQ-0002".to_string()),
        "relates_to not updated"
    );

    Ok(())
}

#[test]
fn task_blocks_appends_blocks() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "A", TaskStatus::Todo);
    let t2 = test_support::make_test_task("RQ-0002", "B", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1, t2])?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "blocks", "RQ-0001", "RQ-0002"]);
    anyhow::ensure!(status.success(), "task blocks failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let a = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task A");
    anyhow::ensure!(
        a.blocks == vec!["RQ-0002"],
        "blocks not updated: {:?}",
        a.blocks
    );

    Ok(())
}

#[test]
fn task_blocks_multiple_targets() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "A", TaskStatus::Todo);
    let t2 = test_support::make_test_task("RQ-0002", "B", TaskStatus::Todo);
    let t3 = test_support::make_test_task("RQ-0003", "C", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1, t2, t3])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "blocks", "RQ-0001", "RQ-0002", "RQ-0003"],
    );
    anyhow::ensure!(status.success(), "task blocks failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let a = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task A");
    anyhow::ensure!(
        a.blocks == vec!["RQ-0002", "RQ-0003"],
        "blocks not updated correctly: {:?}",
        a.blocks
    );

    Ok(())
}

#[test]
fn task_mark_duplicate_sets_duplicates() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "Original", TaskStatus::Todo);
    let t2 = test_support::make_test_task("RQ-0002", "Dupe", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1, t2])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "mark-duplicate", "RQ-0002", "RQ-0001"],
    );
    anyhow::ensure!(status.success(), "mark-duplicate failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let dupe = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("dupe");
    anyhow::ensure!(
        dupe.duplicates.as_deref() == Some("RQ-0001"),
        "duplicates not set"
    );

    Ok(())
}

#[test]
fn task_relate_rejects_invalid_relationship() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "A", TaskStatus::Todo);
    let t2 = test_support::make_test_task("RQ-0002", "B", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1, t2])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "relate", "RQ-0001", "nope", "RQ-0002"],
    );
    anyhow::ensure!(
        !status.success(),
        "expected failure for invalid relationship"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("invalid") || stderr.to_lowercase().contains("unknown"),
        "unexpected stderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn task_relate_fails_on_missing_task() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "A", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "relate", "RQ-0001", "blocks", "RQ-9999"],
    );
    anyhow::ensure!(!status.success(), "expected failure for missing task");
    anyhow::ensure!(
        stderr.to_lowercase().contains("not found") || stderr.to_lowercase().contains("task"),
        "unexpected stderr:\n{stderr}"
    );

    Ok(())
}
