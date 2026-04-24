//! Publish-mode-focused post-run supervision scenarios.
//!
//! Purpose:
//! - Publish-mode-focused post-run supervision scenarios.
//!
//! Responsibilities:
//! - Validate commit, push, and no-op publish behavior once post-run mutation decisions are made.
//!
//! Not handled here:
//! - CI-gate retry/error sequencing.
//! - Queue maintenance repair semantics.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::super::support::{make_task, resolved_for_repo, write_done_tasks, write_queue};
use crate::commands::run::supervision::{PushPolicy, post_run_supervise};
use crate::contracts::{GitPublishMode, GitRevertMode, TaskStatus};
use crate::queue;
use crate::testsupport::git as git_test;
use std::path::Path;
use tempfile::TempDir;

fn write_empty_queue(repo_root: &Path) -> anyhow::Result<()> {
    queue::save_queue(
        &repo_root.join(".ralph/queue.jsonc"),
        &crate::contracts::QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    Ok(())
}

fn configure_tracking_remote(repo_root: &Path) -> anyhow::Result<TempDir> {
    let remote = TempDir::new()?;
    git_test::git_run(remote.path(), &["init", "--bare"])?;
    let branch = git_test::git_output(repo_root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::git_run(
        repo_root,
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )?;
    git_test::git_run(repo_root, &["push", "-u", "origin", &branch])?;
    Ok(remote)
}

fn upstream_counts(repo_root: &Path) -> anyhow::Result<(u32, u32)> {
    let counts = git_test::git_output(
        repo_root,
        &["rev-list", "--left-right", "--count", "@{u}...HEAD"],
    )?;
    let mut parts = counts.split_whitespace();
    let behind = parts
        .next()
        .expect("behind count")
        .parse()
        .expect("numeric behind count");
    let ahead = parts
        .next()
        .expect("ahead count")
        .parse()
        .expect("numeric ahead count");
    Ok((behind, ahead))
}

fn head_commit_subject(repo_root: &Path) -> anyhow::Result<String> {
    git_test::git_output(repo_root, &["log", "-1", "--pretty=%s"])
}

#[test]
fn post_run_supervise_errors_on_push_failure_when_enabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    let remote = TempDir::new()?;
    git_test::git_run(remote.path(), &["init", "--bare"])?;
    let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::git_run(
        temp.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )?;
    git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;
    let missing_remote = temp.path().join("missing-remote");
    git_test::git_run(
        temp.path(),
        &[
            "remote",
            "set-url",
            "origin",
            missing_remote.to_str().unwrap(),
        ],
    )?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    let err = post_run_supervise(
        &resolved,
        None,
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
    )
    .expect_err("expected push failure");
    assert!(format!("{err:#}").contains("Git push failed"));
    Ok(())
}

#[test]
fn post_run_supervise_skips_push_when_disabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    let remote = TempDir::new()?;
    git_test::git_run(remote.path(), &["init", "--bare"])?;
    let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::git_run(
        temp.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )?;
    git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;
    let missing_remote = temp.path().join("missing-remote");
    git_test::git_run(
        temp.path(),
        &[
            "remote",
            "set-url",
            "origin",
            missing_remote.to_str().unwrap(),
        ],
    )?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        None,
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
    Ok(())
}

#[test]
fn post_run_supervise_commit_mode_commits_without_pushing() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    let _remote = configure_tracking_remote(temp.path())?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        None,
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

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(
        status.trim().is_empty(),
        "expected clean repo after commit mode"
    );
    anyhow::ensure!(upstream_counts(temp.path())? == (0, 1));

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    anyhow::ensure!(
        done_file.tasks.iter().any(|task| task.id == "RQ-0001"),
        "expected task archived in done file"
    );
    Ok(())
}

#[test]
fn post_run_supervise_noop_archived_done_commit_and_push_pushes_existing_ahead_commit()
-> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_empty_queue(temp.path())?;
    write_done_tasks(
        temp.path(),
        vec![make_task("RQ-0001", "Archived task", TaskStatus::Done)],
    )?;
    git_test::commit_all(temp.path(), "init")?;
    let _remote = configure_tracking_remote(temp.path())?;

    std::fs::write(temp.path().join("ahead.txt"), "ahead")?;
    git_test::commit_all(temp.path(), "local ahead")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        None,
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

    anyhow::ensure!(upstream_counts(temp.path())? == (0, 0));
    anyhow::ensure!(head_commit_subject(temp.path())? == "local ahead");
    Ok(())
}

#[test]
fn post_run_supervise_noop_archived_done_commit_mode_skips_push_for_existing_ahead_commit()
-> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_empty_queue(temp.path())?;
    write_done_tasks(
        temp.path(),
        vec![make_task("RQ-0001", "Archived task", TaskStatus::Done)],
    )?;
    git_test::commit_all(temp.path(), "init")?;
    let _remote = configure_tracking_remote(temp.path())?;

    std::fs::write(temp.path().join("ahead.txt"), "ahead")?;
    git_test::commit_all(temp.path(), "local ahead")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        None,
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

    anyhow::ensure!(upstream_counts(temp.path())? == (0, 1));
    anyhow::ensure!(head_commit_subject(temp.path())? == "local ahead");
    Ok(())
}

#[test]
fn post_run_supervise_allows_productivity_json_dirty() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Done)?;
    git_test::commit_all(temp.path(), "init")?;

    let cache_dir = temp.path().join(".ralph").join("cache");
    std::fs::create_dir_all(&cache_dir)?;
    std::fs::write(
        cache_dir.join("productivity.json"),
        r#"{"version":1,"total_completed":1}"#,
    )?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        None,
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

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    anyhow::ensure!(
        done_file.tasks.iter().any(|task| task.id == "RQ-0001"),
        "expected task in done archive"
    );

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(
        status.trim().is_empty(),
        "expected clean repo after commit, but found: {status}"
    );
    Ok(())
}
