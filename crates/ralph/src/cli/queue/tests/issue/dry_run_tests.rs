//! Dry-run and preview-path queue issue tests.
//!
//! Purpose:
//! - Dry-run and preview-path queue issue tests.
//!
//! Responsibilities:
//! - Verify single-task preview succeeds without mutating the queue.
//! - Verify publish-many filtering succeeds in preview mode.
//! - Keep preview behavior separate from execute-mode scenarios.
//!
//! Not handled here:
//! - Execute-mode GitHub create/update behavior.
//! - Failure-path coverage unrelated to preview mode.
//! - Fake `gh` executable definitions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Preview-mode tests do not depend on persisted GitHub metadata changes.
//! - Existing test function names remain unchanged.
//! - Bulk preview coverage stays behaviorally equivalent to the prior suite.

use super::{
    base_issue_publish_args, issue_task, resolved_for_dir, run_issue_publish,
    run_issue_publish_many, write_issue_queue_tasks, write_queue,
};
use crate::cli::queue::issue;
use crate::cli::queue::shared::StatusArg;
use crate::contracts::TaskStatus;
use anyhow::Result;
use tempfile::TempDir;

#[test]
fn queue_issue_publish_dry_run_succeeds() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = issue::QueueIssuePublishArgs {
        dry_run: true,
        ..base_issue_publish_args("RQ-0001")
    };

    let result = run_issue_publish(&resolved, true, args);
    assert!(result.is_ok());

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_dry_run_filters() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task("RQ-0001", "Bug task one", TaskStatus::Todo, &["bug"], &[]),
            issue_task("RQ-0002", "Bug task two", TaskStatus::Todo, &["bug"], &[]),
            issue_task("RQ-0003", "Other task", TaskStatus::Doing, &["cli"], &[]),
        ],
    )?;

    let args = issue::QueueIssuePublishManyArgs {
        status: vec![StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: Some("^RQ-0001$".to_string()),
        dry_run: false,
        execute: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    };

    let result = run_issue_publish_many(&resolved, true, args);
    assert!(result.is_ok());

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue.tasks.len(), 3);
    assert_eq!(queue.tasks[0].id, "RQ-0001");

    Ok(())
}
