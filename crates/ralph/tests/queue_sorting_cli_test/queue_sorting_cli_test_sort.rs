//! `queue sort` mutation coverage for persistent task ordering.
//!
//! Purpose:
//! - `queue sort` mutation coverage for persistent task ordering.
//!
//! Responsibilities:
//! - Verify `queue sort` rewrites the queue file in the expected order.
//! - Keep persistent-sort assertions separate from list-only output checks.
//!
//! Non-scope:
//! - Dry-run reporting semantics.
//! - Invalid argument rejection.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Persisted ordering assertions read task IDs from `.ralph/queue.jsonc`.

use super::queue_sorting_cli_test_support::{queue_sort_ids, setup_repo, write_priority_queue};
use anyhow::Result;

#[test]
fn queue_sort_reorders_queue_by_priority_descending() -> Result<()> {
    let dir = setup_repo()?;
    write_priority_queue(dir.path())?;
    let ids = queue_sort_ids(
        dir.path(),
        &[
            "queue",
            "sort",
            "--sort-by",
            "priority",
            "--order",
            "descending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0002", "RQ-0003", "RQ-0001"]);
    Ok(())
}

#[test]
fn queue_sort_reorders_queue_by_priority_ascending() -> Result<()> {
    let dir = setup_repo()?;
    write_priority_queue(dir.path())?;
    let ids = queue_sort_ids(
        dir.path(),
        &[
            "queue",
            "sort",
            "--sort-by",
            "priority",
            "--order",
            "ascending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0001", "RQ-0003", "RQ-0002"]);
    Ok(())
}

#[test]
fn queue_sort_defaults_to_descending_priority() -> Result<()> {
    let dir = setup_repo()?;
    write_priority_queue(dir.path())?;
    let ids = queue_sort_ids(dir.path(), &["queue", "sort", "--sort-by", "priority"])?;
    assert_eq!(ids, ["RQ-0002", "RQ-0003", "RQ-0001"]);
    Ok(())
}
