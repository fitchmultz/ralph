//! Queue/archive-focused post-run supervision scenarios.
//!
//! Responsibilities:
//! - Validate queue-to-done archival, dirty-repo finalization, and terminal-task handling.
//!
//! Not handled here:
//! - CI gate retry/failure sequencing.
//! - Upstream push behavior variants.

use super::super::support::{make_task, resolved_for_repo, write_queue, write_queue_tasks};
use crate::commands::run::supervision::{PushPolicy, post_run_supervise};
use crate::contracts::{GitPublishMode, GitRevertMode, TaskStatus};
use crate::queue;
use crate::testsupport::git as git_test;
use tempfile::TempDir;

#[test]
fn post_run_supervise_commits_and_cleans_when_enabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::CommitAndPush,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(status.trim().is_empty(), "expected clean repo");

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    anyhow::ensure!(
        done_file.tasks.iter().any(|task| task.id == "RQ-0001"),
        "expected task in done archive"
    );
    Ok(())
}

#[test]
fn post_run_supervise_skips_commit_when_disabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(!status.trim().is_empty(), "expected dirty repo");
    Ok(())
}

#[test]
fn post_run_supervise_archives_rejected_terminal_tasks_alongside_completed_task()
-> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue_tasks(
        temp.path(),
        vec![
            make_task("RQ-0001", "Primary task", TaskStatus::Todo),
            make_task("RQ-0002", "Rejected sibling", TaskStatus::Rejected),
        ],
    )?;
    git_test::commit_all(temp.path(), "init")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::Commit,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    anyhow::ensure!(queue_file.tasks.is_empty(), "expected queue to be empty");

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let archived_primary = done_file
        .tasks
        .iter()
        .find(|task| task.id == "RQ-0001")
        .expect("primary task should be archived");
    anyhow::ensure!(archived_primary.status == TaskStatus::Done);

    let archived_rejected = done_file
        .tasks
        .iter()
        .find(|task| task.id == "RQ-0002")
        .expect("rejected sibling should be archived");
    anyhow::ensure!(archived_rejected.status == TaskStatus::Rejected);

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(
        status.trim().is_empty(),
        "expected clean repo after commit mode"
    );
    Ok(())
}

#[test]
fn post_run_supervise_backfills_missing_completed_at() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Done)?;
    git_test::commit_all(temp.path(), "init")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let task = done_file
        .tasks
        .iter()
        .find(|task| task.id == "RQ-0001")
        .expect("expected task in done archive");
    let completed_at = task
        .completed_at
        .as_deref()
        .expect("completed_at should be stamped");

    crate::timeutil::parse_rfc3339(completed_at)?;
    Ok(())
}
