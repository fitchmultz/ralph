//! Read-only queue/report command regression tests.
//!
//! Purpose:
//! - Read-only queue/report command regression tests.
//!
//! Responsibilities:
//! - Verify read-only CLI surfaces do not mutate queue/done files.
//! - Verify read-only CLI surfaces do not leave the repo dirty.
//! - Verify invalid legacy timestamps fail without hidden write-on-read repair.
//!
//! Not handled here:
//! - Mutation commands such as archive/import/repair.
//! - Full report rendering correctness (covered by other tests).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests run in isolated git repos outside the workspace repo.
//! - Queue/done files are committed before commands run so git dirtiness is meaningful.

mod test_support;

use anyhow::{Context, Result};
use ralph::contracts::{Task, TaskStatus};
use std::path::Path;
use std::process::Command;
use test_support::{
    QueueDoneSnapshot, git_add_all_commit, git_init, make_test_task, run_in_dir, seed_ralph_dir,
    snapshot_queue_done, temp_dir_outside_repo, write_done, write_queue,
};

fn git_status_porcelain(dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["status", "--short"])
        .output()
        .context("run git status --short")?;
    anyhow::ensure!(
        output.status.success(),
        "git status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn assert_repo_clean_and_files_unchanged(
    dir: &Path,
    before: &QueueDoneSnapshot,
    context: &str,
) -> Result<()> {
    let after = snapshot_queue_done(dir)?;
    assert_eq!(after, *before, "{context}: queue/done files changed");

    let status = git_status_porcelain(dir)?;
    assert!(status.is_empty(), "{context}: repo became dirty: {status}");

    Ok(())
}

fn make_parent_task() -> Task {
    make_test_task("RQ-0001", "Parent task", TaskStatus::Todo)
}

fn make_child_task() -> Task {
    let mut child = make_test_task("RQ-0002", "Child task", TaskStatus::Todo);
    child.parent_id = Some("RQ-0001".to_string());
    child
}

fn make_done_task() -> Task {
    make_test_task("RQ-0003", "Completed task", TaskStatus::Done)
}

fn setup_valid_repo() -> Result<tempfile::TempDir> {
    let dir = temp_dir_outside_repo();
    git_init(dir.path())?;
    seed_ralph_dir(dir.path())?;
    write_queue(dir.path(), &[make_parent_task(), make_child_task()])?;
    write_done(dir.path(), &[make_done_task()])?;
    git_add_all_commit(dir.path(), "seed queue state")?;
    Ok(dir)
}

fn setup_invalid_repo_with_non_utc_timestamp() -> Result<tempfile::TempDir> {
    let dir = temp_dir_outside_repo();
    git_init(dir.path())?;
    seed_ralph_dir(dir.path())?;

    let mut legacy = make_test_task("RQ-0001", "Legacy task", TaskStatus::Todo);
    legacy.created_at = Some("2026-01-18T12:00:00-05:00".to_string());
    write_queue(dir.path(), &[legacy])?;
    write_done(dir.path(), &[])?;
    git_add_all_commit(dir.path(), "seed legacy queue state")?;
    Ok(dir)
}

#[test]
fn read_only_commands_leave_repo_clean_on_success() -> Result<()> {
    let dir = setup_valid_repo()?;

    let commands: &[&[&str]] = &[
        &["queue", "list", "--include-done"],
        &["queue", "search", "Child", "--include-done"],
        &["queue", "show", "RQ-0002"],
        &["queue", "history", "--days", "7"],
        &["queue", "dashboard", "--days", "7"],
        &["queue", "graph", "--include-done"],
        &["queue", "next"],
        &["queue", "explain", "--format", "json"],
        &["task", "parent", "RQ-0002"],
        &["task", "children", "RQ-0001", "--recursive"],
    ];

    for args in commands {
        let before = snapshot_queue_done(dir.path())?;
        let (status, stdout, stderr) = run_in_dir(dir.path(), args);
        anyhow::ensure!(
            status.success(),
            "command failed: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            args
        );
        assert_repo_clean_and_files_unchanged(
            dir.path(),
            &before,
            &format!("read-only command {:?}", args),
        )?;
    }

    Ok(())
}

#[test]
fn read_only_commands_do_not_rewrite_legacy_timestamps_on_failure() -> Result<()> {
    let dir = setup_invalid_repo_with_non_utc_timestamp()?;

    let commands: &[&[&str]] = &[
        &["queue", "list"],
        &["queue", "search", "Legacy"],
        &["queue", "show", "RQ-0001"],
    ];

    for args in commands {
        let before = snapshot_queue_done(dir.path())?;
        let (status, _stdout, stderr) = run_in_dir(dir.path(), args);
        assert!(
            !status.success(),
            "command unexpectedly succeeded for legacy invalid queue: {:?}",
            args
        );
        assert!(
            stderr.contains("RFC3339 UTC timestamp"),
            "expected validation failure for {:?}, stderr was:\n{stderr}",
            args
        );
        assert_repo_clean_and_files_unchanged(
            dir.path(),
            &before,
            &format!("legacy read-only command {:?}", args),
        )?;
    }

    Ok(())
}
