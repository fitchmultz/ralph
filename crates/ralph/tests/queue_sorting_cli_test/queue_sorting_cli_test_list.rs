//! `queue list` ordering coverage for sorting fields.
//!
//! Purpose:
//! - `queue list` ordering coverage for sorting fields.
//!
//! Responsibilities:
//! - Verify list ordering across priority, timestamps, status, title, and missing-value tie breaks.
//!
//! Non-scope:
//! - Persistent queue mutation via `queue sort`.
//! - Dry-run reporting behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Returned IDs are read from tab-separated CLI output.

use super::queue_sorting_cli_test_support::{
    queue_list_ids, setup_repo, write_extended_sort_queue, write_priority_queue,
};
use anyhow::Result;

#[test]
fn queue_list_sorts_by_priority_descending() -> Result<()> {
    let dir = setup_repo()?;
    write_priority_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
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
fn queue_list_defaults_to_descending_priority() -> Result<()> {
    let dir = setup_repo()?;
    write_priority_queue(dir.path())?;
    let ids = queue_list_ids(dir.path(), &["queue", "list", "--sort-by", "priority"])?;
    assert_eq!(ids, ["RQ-0002", "RQ-0003", "RQ-0001"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_priority_ascending() -> Result<()> {
    let dir = setup_repo()?;
    write_priority_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
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
fn queue_list_sorts_by_created_at_descending() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "created_at",
            "--order",
            "descending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0004", "RQ-0003", "RQ-0002", "RQ-0005", "RQ-0001"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_created_at_ascending() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "created_at",
            "--order",
            "ascending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0001", "RQ-0005", "RQ-0002", "RQ-0003", "RQ-0004"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_updated_at_descending() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "updated_at",
            "--order",
            "descending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0004", "RQ-0003", "RQ-0002", "RQ-0005", "RQ-0001"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_started_at_missing_last() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "started_at",
            "--order",
            "ascending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0005", "RQ-0002", "RQ-0003", "RQ-0001", "RQ-0004"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_started_at_descending_missing_last() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "started_at",
            "--order",
            "descending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0003", "RQ-0002", "RQ-0005", "RQ-0001", "RQ-0004"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_scheduled_start_missing_last() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "scheduled_start",
            "--order",
            "ascending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0002", "RQ-0005", "RQ-0003", "RQ-0001", "RQ-0004"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_scheduled_start_descending_missing_last() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "scheduled_start",
            "--order",
            "descending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0003", "RQ-0005", "RQ-0002", "RQ-0001", "RQ-0004"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_status_ascending() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "status",
            "--order",
            "ascending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0001", "RQ-0002", "RQ-0003", "RQ-0004", "RQ-0005"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_status_descending() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "status",
            "--order",
            "descending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0005", "RQ-0004", "RQ-0003", "RQ-0002", "RQ-0001"]);
    Ok(())
}

#[test]
fn queue_list_sorts_by_title_case_insensitive_ascending() -> Result<()> {
    let dir = setup_repo()?;
    write_extended_sort_queue(dir.path())?;
    let ids = queue_list_ids(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "title",
            "--order",
            "ascending",
        ],
    )?;
    assert_eq!(ids, ["RQ-0002", "RQ-0003", "RQ-0005", "RQ-0004", "RQ-0001"]);
    Ok(())
}
