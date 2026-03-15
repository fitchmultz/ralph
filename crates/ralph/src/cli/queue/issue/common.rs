//! Shared publish-mode, filtering, and queue lookup helpers for `ralph queue issue`.
//!
//! Responsibilities:
//! - Define internal publish result/filter/summary types.
//! - Parse and validate bulk publish filters.
//! - Provide queue/task lookup helpers shared by publish and rendering paths.
//!
//! Not handled here:
//! - GitHub issue creation/update side effects.
//! - Interactive prompt rendering.
//! - Clap type definitions.
//!
//! Invariants/assumptions:
//! - Status filters default to the established non-draft publishable statuses.
//! - Task lookup uses trimmed IDs and returns the canonical queue task.
//! - Empty custom-field values are treated as absent.

use anyhow::{Context, Result, bail};
use regex::Regex;
use std::collections::{HashMap, HashSet};

use crate::contracts::{QueueFile, Task, TaskStatus};

use super::args::QueueIssuePublishManyArgs;

pub(super) const DEFAULT_PUBLISH_STATUSES: &[TaskStatus] = &[
    TaskStatus::Todo,
    TaskStatus::Doing,
    TaskStatus::Done,
    TaskStatus::Rejected,
];
pub(super) const GITHUB_ISSUE_URL_KEY: &str = "github_issue_url";
pub(super) const GITHUB_ISSUE_NUMBER_KEY: &str = "github_issue_number";
pub(super) use crate::git::GITHUB_ISSUE_SYNC_HASH_KEY;

#[derive(Debug, Clone, Copy)]
pub(super) enum PublishMode {
    DryRun,
    Execute,
}

#[derive(Debug)]
pub(super) enum PublishItemResult {
    Created,
    Updated,
    SkippedUnchanged,
    Failed(anyhow::Error),
}

impl PublishItemResult {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Created => "CREATE",
            Self::Updated => "UPDATE",
            Self::SkippedUnchanged => "SKIP",
            Self::Failed(_) => "ERROR",
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct PublishManySummary {
    pub(super) selected: usize,
    pub(super) created: usize,
    pub(super) updated: usize,
    pub(super) skipped: usize,
    pub(super) failed: usize,
}

impl PublishManySummary {
    pub(super) fn has_mutations(&self) -> bool {
        self.created > 0 || self.updated > 0
    }
}

#[derive(Debug)]
pub(super) struct PublishManyFilters {
    pub(super) statuses: Vec<TaskStatus>,
    pub(super) tags: Vec<String>,
    pub(super) id_pattern: Option<Regex>,
}

pub(super) fn resolve_publish_mode(dry_run: bool, execute: bool) -> Result<PublishMode> {
    if dry_run && execute {
        bail!("Cannot combine --dry-run and --execute");
    }
    if execute {
        Ok(PublishMode::Execute)
    } else {
        Ok(PublishMode::DryRun)
    }
}

pub(super) fn parse_publish_many_filters(
    args: &QueueIssuePublishManyArgs,
) -> Result<PublishManyFilters> {
    let statuses = if args.status.is_empty() {
        DEFAULT_PUBLISH_STATUSES.to_vec()
    } else {
        args.status.iter().map(|status| (*status).into()).collect()
    };

    let tags = args
        .tag
        .iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();

    let id_pattern = match args.id_pattern.as_deref() {
        Some(pattern) if !pattern.trim().is_empty() => {
            Some(Regex::new(pattern).with_context(|| {
                format!("Invalid --id-pattern '{pattern}'. Use valid regular-expression syntax.")
            })?)
        }
        Some(pattern) if pattern.trim().is_empty() => {
            bail!("--id-pattern cannot be empty when provided");
        }
        Some(_) => unreachable!(),
        None => None,
    };

    Ok(PublishManyFilters {
        statuses,
        tags,
        id_pattern,
    })
}

pub(super) fn select_publishable_task_ids(
    queue_file: &QueueFile,
    filters: &PublishManyFilters,
) -> Vec<String> {
    let status_filter: HashSet<TaskStatus> = filters.statuses.iter().copied().collect();
    let statuses = status_filter.into_iter().collect::<Vec<_>>();
    let tasks = crate::queue::filter_tasks(queue_file, &statuses, &filters.tags, &[], None);

    tasks
        .into_iter()
        .filter(|task| {
            filters
                .id_pattern
                .as_ref()
                .is_none_or(|pattern| pattern.is_match(task.id.trim()))
        })
        .map(|task| task.id.trim().to_string())
        .collect()
}

pub(super) fn accumulate_publish_result(
    summary: &mut PublishManySummary,
    result: &PublishItemResult,
) {
    match result {
        PublishItemResult::Created => summary.created += 1,
        PublishItemResult::Updated => summary.updated += 1,
        PublishItemResult::SkippedUnchanged => summary.skipped += 1,
        PublishItemResult::Failed(_) => summary.failed += 1,
    }
}

pub(super) fn fetch_custom_field(
    custom_fields: &HashMap<String, String>,
    key: &str,
) -> Option<String> {
    custom_fields
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(super) fn find_task<'a>(queue: &'a QueueFile, task_id: &str) -> Result<&'a Task> {
    let task_id = task_id.trim();
    queue
        .tasks
        .iter()
        .find(|task| task.id.trim() == task_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue(task_id)
            )
        })
}

pub(super) fn find_task_mut<'a>(queue: &'a mut QueueFile, task_id: &str) -> Result<&'a mut Task> {
    let task_id = task_id.trim();
    queue
        .tasks
        .iter_mut()
        .find(|task| task.id.trim() == task_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue(task_id)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::queue::shared::StatusArg;
    use crate::contracts::{QueueFile, Task};

    fn queue_with_ids(ids: &[(&str, TaskStatus, &[&str])]) -> QueueFile {
        QueueFile {
            version: 1,
            tasks: ids
                .iter()
                .map(|(id, status, tags)| Task {
                    id: (*id).to_string(),
                    status: *status,
                    title: format!("Task {id}"),
                    description: None,
                    priority: Default::default(),
                    tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
                    scope: vec![],
                    evidence: vec![],
                    plan: vec![],
                    notes: vec![],
                    request: None,
                    agent: None,
                    created_at: None,
                    updated_at: None,
                    completed_at: None,
                    started_at: None,
                    scheduled_start: None,
                    depends_on: vec![],
                    blocks: vec![],
                    relates_to: vec![],
                    duplicates: None,
                    custom_fields: HashMap::new(),
                    parent_id: None,
                    estimated_minutes: None,
                    actual_minutes: None,
                })
                .collect(),
        }
    }

    #[test]
    fn resolve_publish_mode_rejects_conflicting_flags() {
        let err = resolve_publish_mode(true, true).expect_err("expected conflict");
        assert!(
            err.to_string()
                .contains("Cannot combine --dry-run and --execute")
        );
    }

    #[test]
    fn parse_publish_many_filters_rejects_empty_pattern() {
        let args = QueueIssuePublishManyArgs {
            status: vec![StatusArg::Todo],
            tag: vec![],
            id_pattern: Some("   ".to_string()),
            dry_run: false,
            execute: false,
            label: vec![],
            assignee: vec![],
            repo: None,
        };

        let err = parse_publish_many_filters(&args).expect_err("expected empty pattern error");
        assert!(err.to_string().contains("--id-pattern cannot be empty"));
    }

    #[test]
    fn select_publishable_task_ids_trims_and_filters() {
        let queue = queue_with_ids(&[
            (" RQ-0001 ", TaskStatus::Todo, &["bug"]),
            ("RQ-0002", TaskStatus::Doing, &["bug"]),
            ("RQ-0003", TaskStatus::Todo, &["ops"]),
        ]);
        let filters = PublishManyFilters {
            statuses: vec![TaskStatus::Todo],
            tags: vec!["bug".to_string()],
            id_pattern: Some(Regex::new("0001$").expect("regex")),
        };

        let selected = select_publishable_task_ids(&queue, &filters);
        assert_eq!(selected, vec!["RQ-0001"]);
    }
}
