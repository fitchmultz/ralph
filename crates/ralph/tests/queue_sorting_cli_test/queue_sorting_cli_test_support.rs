//! Shared fixtures and command helpers for queue sorting CLI integration tests.
//!
//! Purpose:
//! - Shared fixtures and command helpers for queue sorting CLI integration tests.
//!
//! Responsibilities:
//! - Initialize disposable Ralph repos for sorting scenarios.
//! - Build canonical priority and timestamp-rich queue fixtures using shared task builders.
//! - Provide stable helpers for running sort/list commands and reading task ordering assertions.
//!
//! Non-scope:
//! - Scenario assertions for specific sort modes.
//! - Queue sorting implementation logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Fixture queues always live at `.ralph/queue.jsonc`.
//! - Helpers expect `queue list` output to stay tab-separated.

use crate::test_support;
use anyhow::{Context, Result};
use ralph::contracts::{TaskPriority, TaskStatus};
use std::path::Path;
use std::process::ExitStatus;
use tempfile::TempDir;

pub(super) fn setup_repo() -> Result<TempDir> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::seed_ralph_dir(dir.path())?;
    Ok(dir)
}

pub(super) fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
}

pub(super) fn write_priority_queue(dir: &Path) -> Result<()> {
    test_support::write_queue_file(
        dir,
        [
            test_support::TaskBuilder::new(
                "RQ-0001",
                "Low priority",
                TaskStatus::Todo,
                TaskPriority::Low,
            )
            .tags(&["cli"])
            .scope(&["crates/ralph"])
            .evidence(&["test"])
            .plan(&["verify"])
            .request("test")
            .created_at("2026-01-18T00:00:00Z")
            .updated_at("2026-01-18T00:00:00Z")
            .build(),
            test_support::TaskBuilder::new(
                "RQ-0002",
                "Critical priority",
                TaskStatus::Todo,
                TaskPriority::Critical,
            )
            .tags(&["cli"])
            .scope(&["crates/ralph"])
            .evidence(&["test"])
            .plan(&["verify"])
            .request("test")
            .created_at("2026-01-18T00:00:00Z")
            .updated_at("2026-01-18T00:00:00Z")
            .build(),
            test_support::TaskBuilder::new(
                "RQ-0003",
                "High priority",
                TaskStatus::Todo,
                TaskPriority::High,
            )
            .tags(&["cli"])
            .scope(&["crates/ralph"])
            .evidence(&["test"])
            .plan(&["verify"])
            .request("test")
            .created_at("2026-01-18T00:00:00Z")
            .updated_at("2026-01-18T00:00:00Z")
            .build(),
        ],
    )
}

pub(super) fn write_extended_sort_queue(dir: &Path) -> Result<()> {
    test_support::write_queue_file(
        dir,
        [
            test_support::TaskBuilder::new(
                "RQ-0001",
                "Zebra task",
                TaskStatus::Draft,
                TaskPriority::Low,
            )
            .tags(&["test"])
            .created_at("2026-01-10T00:00:00Z")
            .updated_at("2026-01-15T00:00:00Z")
            .build(),
            test_support::TaskBuilder::new(
                "RQ-0002",
                "Alpha task",
                TaskStatus::Todo,
                TaskPriority::Medium,
            )
            .tags(&["test"])
            .created_at("2026-01-15T00:00:00Z")
            .updated_at("2026-01-20T00:00:00Z")
            .started_at(Some("2026-01-16T00:00:00Z"))
            .scheduled_start(Some("2026-02-01T10:00:00Z"))
            .build(),
            test_support::TaskBuilder::new(
                "RQ-0003",
                "beta task",
                TaskStatus::Doing,
                TaskPriority::High,
            )
            .tags(&["test"])
            .created_at("2026-01-20T00:00:00Z")
            .updated_at("2026-01-25T00:00:00Z")
            .started_at(Some("2026-01-21T00:00:00Z"))
            .scheduled_start(Some("2026-02-05T14:00:00Z"))
            .build(),
            test_support::TaskBuilder::new(
                "RQ-0004",
                "GAMMA TASK",
                TaskStatus::Done,
                TaskPriority::Critical,
            )
            .tags(&["test"])
            .created_at("2026-01-25T00:00:00Z")
            .updated_at("2026-01-30T00:00:00Z")
            .completed_at(Some("2026-01-30T00:00:00Z"))
            .started_at(Some("invalid-timestamp"))
            .scheduled_start(Some("not-a-timestamp"))
            .build(),
            test_support::TaskBuilder::new(
                "RQ-0005",
                "delta task",
                TaskStatus::Rejected,
                TaskPriority::Low,
            )
            .tags(&["test"])
            .created_at("2026-01-12T00:00:00Z")
            .updated_at("2026-01-18T00:00:00Z")
            .completed_at(Some("2026-01-18T00:00:00Z"))
            .started_at(Some("2026-01-13T00:00:00Z"))
            .scheduled_start(Some("2026-02-03T09:00:00Z"))
            .build(),
        ],
    )
}

pub(super) fn write_already_sorted_priority_queue(dir: &Path) -> Result<()> {
    test_support::write_queue_file(
        dir,
        [
            test_support::TaskBuilder::new(
                "RQ-0001",
                "Critical priority",
                TaskStatus::Todo,
                TaskPriority::Critical,
            )
            .tags(&["cli"])
            .scope(&["crates/ralph"])
            .evidence(&["test"])
            .plan(&["verify"])
            .request("test")
            .created_at("2026-01-18T00:00:00Z")
            .updated_at("2026-01-18T00:00:00Z")
            .build(),
            test_support::TaskBuilder::new(
                "RQ-0002",
                "Low priority",
                TaskStatus::Todo,
                TaskPriority::Low,
            )
            .tags(&["cli"])
            .scope(&["crates/ralph"])
            .evidence(&["test"])
            .plan(&["verify"])
            .request("test")
            .created_at("2026-01-18T00:00:00Z")
            .updated_at("2026-01-18T00:00:00Z")
            .build(),
        ],
    )
}

pub(super) fn assert_success(
    status: ExitStatus,
    stdout: &str,
    stderr: &str,
    context: &str,
) -> Result<()> {
    anyhow::ensure!(
        status.success(),
        "{context} failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Ok(())
}

pub(super) fn queue_list_ids(dir: &Path, args: &[&str]) -> Result<Vec<String>> {
    let (status, stdout, stderr) = run_in_dir(dir, args);
    assert_success(status, &stdout, &stderr, "queue list")?;
    Ok(test_support::tab_separated_ids(&stdout))
}

pub(super) fn queue_sort_ids(dir: &Path, args: &[&str]) -> Result<Vec<String>> {
    let (status, stdout, stderr) = run_in_dir(dir, args);
    assert_success(status, &stdout, &stderr, "queue sort")?;
    test_support::read_queue_task_ids(dir)
}

pub(super) fn queue_sort_output(dir: &Path, args: &[&str]) -> Result<String> {
    let (status, stdout, stderr) = run_in_dir(dir, args);
    assert_success(status, &stdout, &stderr, "queue sort")?;
    Ok(format!("{stdout}\n{stderr}"))
}

pub(super) fn read_queue_json(dir: &Path) -> Result<String> {
    let path = dir.join(".ralph/queue.jsonc");
    std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}
