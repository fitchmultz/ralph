//! Shared queue issue test helpers.
//!
//! Purpose:
//! - Shared queue issue test helpers.
//!
//! Responsibilities:
//! - Provide reusable argument builders and command wrappers for issue publish tests.
//! - Build queue-task fixtures with consistent defaults.
//! - Write focused queue files used by issue publish scenarios.
//!
//! Not handled here:
//! - Test assertions.
//! - Fake `gh` executable generation.
//! - CLI help smoke coverage.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Helpers preserve the prior queue issue test behavior.
//! - Task fixtures remain valid against the queue contract schema.
//! - Visibility stays limited to the issue test suite.

use crate::cli::queue::issue;
use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

pub(super) fn base_issue_publish_args(task_id: &str) -> issue::QueueIssuePublishArgs {
    issue::QueueIssuePublishArgs {
        task_id: task_id.to_string(),
        dry_run: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    }
}

pub(super) fn run_issue_publish(
    resolved: &crate::config::Resolved,
    force: bool,
    args: issue::QueueIssuePublishArgs,
) -> Result<()> {
    issue::handle(
        resolved,
        force,
        issue::QueueIssueArgs {
            command: issue::QueueIssueCommand::Publish(args),
        },
    )
}

pub(super) fn run_issue_publish_many(
    resolved: &crate::config::Resolved,
    force: bool,
    args: issue::QueueIssuePublishManyArgs,
) -> Result<()> {
    issue::handle(
        resolved,
        force,
        issue::QueueIssueArgs {
            command: issue::QueueIssueCommand::PublishMany(args),
        },
    )
}

pub(super) fn issue_task(
    id: &str,
    title: &str,
    status: TaskStatus,
    tags: &[&str],
    custom_fields: &[(&str, &str)],
) -> Task {
    let mut fields = HashMap::new();
    for (key, value) in custom_fields {
        fields.insert((*key).to_string(), (*value).to_string());
    }

    Task {
        id: id.to_string(),
        status,
        title: title.to_string(),
        description: None,
        priority: Default::default(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test".to_string()],
        plan: vec!["verify".to_string()],
        notes: vec![],
        request: Some("test".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: fields,
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    }
}

pub(super) fn write_issue_queue_tasks(path: &Path, tasks: Vec<Task>) -> Result<()> {
    let queue = QueueFile { version: 1, tasks };
    let rendered = serde_json::to_string_pretty(&queue)?;
    std::fs::write(path, rendered)?;
    Ok(())
}
