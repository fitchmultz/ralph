//! Dry-run reporting coverage for `queue sort`.
//!
//! Purpose:
//! - Dry-run reporting coverage for `queue sort`.
//!
//! Responsibilities:
//! - Verify dry-run mode reports proposed ordering without mutating queue files.
//! - Cover already-sorted messaging regressions.
//!
//! Non-scope:
//! - Persistent `queue sort` mutations or `queue list` output ordering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Dry-run tests compare raw queue file contents before and after execution.

use super::queue_sorting_cli_test_support::{
    queue_sort_output, read_queue_json, setup_repo, write_already_sorted_priority_queue,
    write_priority_queue,
};
use anyhow::Result;

#[test]
fn queue_sort_dry_run_does_not_modify_file() -> Result<()> {
    let dir = setup_repo()?;
    write_priority_queue(dir.path())?;

    let before_queue = read_queue_json(dir.path())?;
    let output = queue_sort_output(dir.path(), &["queue", "sort", "--dry-run"])?;
    anyhow::ensure!(
        output.contains("Dry run"),
        "expected dry-run message, got:\n{output}"
    );

    let after_queue = read_queue_json(dir.path())?;
    anyhow::ensure!(
        before_queue == after_queue,
        "queue.json changed during dry-run"
    );
    Ok(())
}

#[test]
fn queue_sort_dry_run_shows_new_order() -> Result<()> {
    let dir = setup_repo()?;
    write_priority_queue(dir.path())?;
    let output = queue_sort_output(dir.path(), &["queue", "sort", "--dry-run"])?;
    anyhow::ensure!(
        output.contains("RQ-0002") && output.contains("RQ-0003") && output.contains("RQ-0001"),
        "expected task IDs in new order, got:\n{output}"
    );
    Ok(())
}

#[test]
fn queue_sort_dry_run_already_sorted() -> Result<()> {
    let dir = setup_repo()?;
    write_already_sorted_priority_queue(dir.path())?;
    let output = queue_sort_output(dir.path(), &["queue", "sort", "--dry-run"])?;
    anyhow::ensure!(
        output.contains("no changes") || output.contains("already sorted"),
        "expected 'already sorted' or 'no changes' message, got:\n{output}"
    );
    Ok(())
}
