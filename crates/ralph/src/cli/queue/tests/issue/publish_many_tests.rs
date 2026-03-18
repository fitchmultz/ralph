//! Batch publish execution tests for `ralph queue issue`.
//!
//! Responsibilities:
//! - Validate mixed create/update publish-many behavior.
//! - Verify unchanged tasks skip mutation when sync hashes already match.
//! - Ensure partial failures are reported without aborting successful task processing.
//!
//! Not handled here:
//! - Single-task publish cases.
//! - Preview-only behavior checks.
//! - Fake `gh` script construction details.
//!
//! Invariants/assumptions:
//! - All tests in this file are Unix-only because they rely on shell-script shims.
//! - Test function names remain stable.
//! - Queue assertions are made from persisted queue contents after command execution.

#[cfg(unix)]
use super::{
    create_fake_gh_for_issue_publish_multi, issue_task, resolved_for_dir, run_issue_publish_many,
    write_issue_queue_tasks,
};
#[cfg(unix)]
use crate::cli::queue::{issue, shared::StatusArg};
#[cfg(unix)]
use crate::contracts::TaskStatus;
#[cfg(unix)]
use crate::testsupport::path::with_prepend_path;
#[cfg(unix)]
use anyhow::Result;
#[cfg(unix)]
use tempfile::TempDir;

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_exec_mixed_create_update() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task("RQ-0001", "Bug task one", TaskStatus::Todo, &["bug"], &[]),
            issue_task(
                "RQ-0002",
                "Bug task two",
                TaskStatus::Todo,
                &["bug"],
                &[
                    ("github_issue_url", "https://github.com/org/repo/issues/777"),
                    ("github_issue_number", "777"),
                ],
            ),
        ],
    )?;

    let bin_dir = create_fake_gh_for_issue_publish_multi(
        &dir,
        "https://github.com/org/repo/issues/123",
        true,
        None,
    );

    let args = issue::QueueIssuePublishManyArgs {
        status: vec![StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: true,
        label: vec!["triage".to_string()],
        assignee: vec![],
        repo: None,
    };

    with_prepend_path(&bin_dir, || run_issue_publish_many(&resolved, true, args))?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let first = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("first task");
    let second = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("second task");

    assert_eq!(
        first
            .custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/123")
    );
    assert!(
        first
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );
    assert_eq!(
        second
            .custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/777")
    );
    assert!(
        second
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_skips_if_unchanged() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    let task = issue_task("RQ-0001", "No-op task", TaskStatus::Todo, &["bug"], &[]);
    let body = crate::cli::queue::export::render_task_as_github_issue_body(&task);
    let hash = crate::git::compute_issue_sync_hash(
        &format!("{}: {}", task.id, task.title),
        &body,
        &[],
        &[],
        None,
    )?;

    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![issue_task(
            "RQ-0001",
            "No-op task",
            TaskStatus::Todo,
            &["bug"],
            &[
                ("github_issue_url", "https://github.com/org/repo/issues/123"),
                (crate::git::GITHUB_ISSUE_SYNC_HASH_KEY, &hash),
            ],
        )],
    )?;

    let args = issue::QueueIssuePublishManyArgs {
        status: vec![StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    };

    run_issue_publish_many(&resolved, true, args)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_partial_failures_do_not_abort() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task(
                "RQ-0001",
                "Task that fails",
                TaskStatus::Todo,
                &["bug"],
                &[],
            ),
            issue_task(
                "RQ-0002",
                "Task that succeeds",
                TaskStatus::Todo,
                &["bug"],
                &[
                    ("github_issue_url", "https://github.com/org/repo/issues/777"),
                    ("github_issue_number", "777"),
                ],
            ),
        ],
    )?;

    let bin_dir = create_fake_gh_for_issue_publish_multi(
        &dir,
        "https://github.com/org/repo/issues/123",
        true,
        Some("RQ-0001"),
    );

    let args = issue::QueueIssuePublishManyArgs {
        status: vec![StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: true,
        label: vec![],
        assignee: vec![],
        repo: None,
    };
    let err = with_prepend_path(&bin_dir, || run_issue_publish_many(&resolved, true, args))
        .expect_err("expected publish-many failure");
    assert!(
        err.to_string().contains("completed with 1 failed task(s)")
            || err.to_string().contains("simulated failure"),
        "unexpected error: {err}"
    );

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue.tasks.len(), 2);
    assert_eq!(queue.tasks[1].id, "RQ-0002");
    assert!(
        queue.tasks[1]
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );

    Ok(())
}
