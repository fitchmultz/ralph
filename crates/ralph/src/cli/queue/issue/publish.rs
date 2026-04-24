//! GitHub issue create/update workflow for `ralph queue issue`.
//!
//! Purpose:
//! - GitHub issue create/update workflow for `ralph queue issue`.
//!
//! Responsibilities:
//! - Render task issue payloads and compute sync hashes.
//! - Create or update GitHub issues through the shared git/gh helpers.
//! - Persist GitHub metadata back into queue task custom fields during execute mode.
//!
//! Not handled here:
//! - Queue lock acquisition or save orchestration.
//! - Dry-run output formatting.
//! - Bulk selection/filter parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The caller passes a mutable queue loaded from the active repo.
//! - Execute mode writes task metadata only after the GitHub action succeeds.
//! - Sync hashes must reflect title/body/labels/assignees/repo consistently.

use anyhow::{Context, Result};

use crate::config::Resolved;
use crate::contracts::{QueueFile, Task};
use crate::git::{
    compute_issue_sync_hash, create_issue, edit_issue, normalize_issue_metadata_list,
    parse_issue_number,
};

use super::common::{
    GITHUB_ISSUE_NUMBER_KEY, GITHUB_ISSUE_SYNC_HASH_KEY, GITHUB_ISSUE_URL_KEY, PublishItemResult,
    PublishMode, fetch_custom_field, find_task_mut,
};

pub(super) fn publish_task(
    resolved: &Resolved,
    queue: &mut QueueFile,
    task_id: &str,
    mode: PublishMode,
    labels: &[String],
    assignees: &[String],
    repo: Option<&str>,
) -> Result<PublishItemResult> {
    let normalized_labels = normalize_issue_metadata_list(labels);
    let normalized_assignees = normalize_issue_metadata_list(assignees);

    let task = find_task_mut(queue, task_id)?;
    let payload = IssuePayload::new(task, &normalized_labels, &normalized_assignees, repo)?;

    match fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_URL_KEY) {
        Some(url) => update_existing_issue(resolved, task, &payload, mode, &url, repo),
        None => create_new_issue(resolved, task, &payload, mode, repo),
    }
}

struct IssuePayload {
    title: String,
    body: String,
    sync_hash: String,
    normalized_labels: Vec<String>,
    normalized_assignees: Vec<String>,
}

impl IssuePayload {
    fn new(
        task: &Task,
        normalized_labels: &[String],
        normalized_assignees: &[String],
        repo: Option<&str>,
    ) -> Result<Self> {
        let title = format!("{}: {}", task.id.trim(), task.title);
        let body = super::super::export::render_task_as_github_issue_body(task);
        let sync_hash =
            compute_issue_sync_hash(&title, &body, normalized_labels, normalized_assignees, repo)?;

        Ok(Self {
            title,
            body,
            sync_hash,
            normalized_labels: normalized_labels.to_vec(),
            normalized_assignees: normalized_assignees.to_vec(),
        })
    }
}

fn update_existing_issue(
    resolved: &Resolved,
    task: &mut Task,
    payload: &IssuePayload,
    mode: PublishMode,
    url: &str,
    repo: Option<&str>,
) -> Result<PublishItemResult> {
    let existing_sync_hash = fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_SYNC_HASH_KEY);
    if existing_sync_hash.as_deref() == Some(payload.sync_hash.as_str()) {
        return Ok(PublishItemResult::SkippedUnchanged);
    }

    if matches!(mode, PublishMode::DryRun) {
        return Ok(PublishItemResult::Updated);
    }

    let tmp = crate::fsutil::create_ralph_temp_file("issue")
        .context("create temp file for issue body")?;
    std::fs::write(tmp.path(), &payload.body).context("write issue body to temp file")?;
    edit_issue(
        &resolved.repo_root,
        repo,
        url,
        &payload.title,
        tmp.path(),
        &payload.normalized_labels,
        &payload.normalized_assignees,
    )
    .with_context(|| format!("Failed to update GitHub issue at {url}"))?;

    if fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_NUMBER_KEY).is_none()
        && let Some(number) = parse_issue_number(url)
    {
        task.custom_fields
            .insert(GITHUB_ISSUE_NUMBER_KEY.to_string(), number.to_string());
    }

    persist_issue_metadata(task, Some(url.to_string()), None, &payload.sync_hash);
    Ok(PublishItemResult::Updated)
}

fn create_new_issue(
    resolved: &Resolved,
    task: &mut Task,
    payload: &IssuePayload,
    mode: PublishMode,
    repo: Option<&str>,
) -> Result<PublishItemResult> {
    if matches!(mode, PublishMode::DryRun) {
        return Ok(PublishItemResult::Created);
    }

    let tmp = crate::fsutil::create_ralph_temp_file("issue")
        .context("create temp file for issue body")?;
    std::fs::write(tmp.path(), &payload.body).context("write issue body to temp file")?;
    let issue = create_issue(
        &resolved.repo_root,
        repo,
        &payload.title,
        tmp.path(),
        &payload.normalized_labels,
        &payload.normalized_assignees,
    )?;

    persist_issue_metadata(task, Some(issue.url), issue.number, &payload.sync_hash);
    Ok(PublishItemResult::Created)
}

fn persist_issue_metadata(
    task: &mut Task,
    issue_url: Option<String>,
    issue_number: Option<u32>,
    sync_hash: &str,
) {
    if let Some(url) = issue_url {
        task.custom_fields
            .insert(GITHUB_ISSUE_URL_KEY.to_string(), url);
    }
    if let Some(number) = issue_number {
        task.custom_fields
            .insert(GITHUB_ISSUE_NUMBER_KEY.to_string(), number.to_string());
    }
    task.custom_fields.insert(
        GITHUB_ISSUE_SYNC_HASH_KEY.to_string(),
        sync_hash.to_string(),
    );
    task.updated_at = Some(crate::timeutil::now_utc_rfc3339_or_fallback());
}
