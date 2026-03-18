//! Successful single-task create/update queue issue tests.
//!
//! Responsibilities:
//! - Verify issue creation persists GitHub metadata and updates timestamps.
//! - Verify existing issue updates backfill the stored issue number.
//! - Keep successful execute-mode cases isolated from failure and preview scenarios.
//!
//! Not handled here:
//! - Publish-many behavior.
//! - Preview-only coverage.
//! - Fake `gh` shell-script definitions.
//!
//! Invariants/assumptions:
//! - All tests in this file are Unix-only because they rely on shell-script shims.
//! - Test function names remain stable.
//! - Persisted queue assertions reflect production create/update workflows.

#[cfg(unix)]
use super::{
    base_issue_publish_args, create_fake_gh_for_issue_publish, issue_task, resolved_for_dir,
    run_issue_publish, write_issue_queue_tasks, write_queue,
};
#[cfg(unix)]
use crate::cli::queue::issue;
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
fn queue_issue_publish_creates_issue_and_persists_custom_fields() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/123",
        true,
    );

    let args = issue::QueueIssuePublishArgs {
        label: vec!["bug".to_string()],
        assignee: vec!["@me".to_string()],
        ..base_issue_publish_args("RQ-0001")
    };

    with_prepend_path(&bin_dir, || run_issue_publish(&resolved, true, args))?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let task = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task");

    assert_eq!(
        task.custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/123")
    );
    assert_eq!(
        task.custom_fields
            .get("github_issue_number")
            .map(String::as_str),
        Some("123")
    );
    assert!(
        task.updated_at.as_deref() != Some("2026-01-18T00:00:00Z"),
        "updated_at should be updated on publish"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_updates_existing_issue_and_backfills_issue_number() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![issue_task(
            "RQ-0001",
            "Test task",
            TaskStatus::Todo,
            &["cli"],
            &[("github_issue_url", "https://github.com/org/repo/issues/777")],
        )],
    )?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/777",
        true,
    );

    let args = issue::QueueIssuePublishArgs {
        label: vec!["help-wanted".to_string()],
        assignee: vec!["@me".to_string()],
        ..base_issue_publish_args("RQ-0001")
    };

    with_prepend_path(&bin_dir, || run_issue_publish(&resolved, true, args))?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let task = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task");

    assert_eq!(
        task.custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/777")
    );
    assert_eq!(
        task.custom_fields
            .get("github_issue_number")
            .map(String::as_str),
        Some("777")
    );

    Ok(())
}
