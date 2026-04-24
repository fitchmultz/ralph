//! Watch-task reconciliation helpers.
//!
//! Purpose:
//! - Watch-task reconciliation helpers.
//!
//! Responsibilities:
//! - Match detected comments against active watch tasks.
//! - Upgrade legacy structured metadata when the same comment still exists.
//! - Close active watch tasks only for processed files whose comments disappeared.
//!
//! Not handled here:
//! - Task construction for new comments.
//! - Watch-loop or file-watching orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Title or note text is never used for deduplication.
//! - Only processed files are eligible for auto-close reconciliation.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::commands::watch::identity::{
    ParsedWatchIdentity, WatchCommentIdentity, parse_task_watch_identity, path_key,
};
use crate::commands::watch::types::DetectedComment;
use crate::contracts::{QueueFile, Task, TaskStatus};

#[cfg(test)]
use crate::commands::watch::types::WatchOptions;
#[cfg(test)]
use crate::config::Resolved;
#[cfg(test)]
use crate::queue::{load_queue, save_queue};
#[cfg(test)]
use crate::timeutil;
#[cfg(test)]
use anyhow::{Context, Result};

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn reconcile_watch_tasks(
    resolved: &Resolved,
    detected_comments: &[DetectedComment],
    processed_files: &[PathBuf],
    _opts: &WatchOptions,
) -> Result<Vec<String>> {
    let mut queue = load_queue(&resolved.queue_path)
        .with_context(|| format!("load queue {}", resolved.queue_path.display()))?;
    let now = timeutil::now_utc_rfc3339_or_fallback();
    let closed =
        reconcile_watch_tasks_in_queue(&mut queue, detected_comments, processed_files, &now);

    if !closed.is_empty() {
        save_queue(&resolved.queue_path, &queue)
            .with_context(|| format!("save queue {}", resolved.queue_path.display()))?;
    }

    Ok(closed)
}

pub(super) fn reconcile_watch_tasks_in_queue(
    queue: &mut QueueFile,
    detected_comments: &[DetectedComment],
    processed_files: &[PathBuf],
    now: &str,
) -> Vec<String> {
    let processed_files: HashSet<String> =
        processed_files.iter().map(|path| path_key(path)).collect();
    let current_comments: Vec<WatchCommentIdentity> = detected_comments
        .iter()
        .map(WatchCommentIdentity::from_detected_comment)
        .collect();
    let current_identity_keys: HashSet<&str> = current_comments
        .iter()
        .map(|identity| identity.identity_key.as_str())
        .collect();
    let mut closed = Vec::new();

    for task in &mut queue.tasks {
        if !is_active_watch_task(task) {
            continue;
        }

        let Some(parsed_identity) = parse_task_watch_identity(task) else {
            continue;
        };
        let task_file = match &parsed_identity {
            ParsedWatchIdentity::V2(identity) => identity.file.as_str(),
            ParsedWatchIdentity::LegacyStructured(identity) => identity.file.as_str(),
            ParsedWatchIdentity::LegacyUnstructured => continue,
        };
        if !processed_files.contains(task_file) {
            continue;
        }

        let comment_still_exists = match &parsed_identity {
            ParsedWatchIdentity::V2(identity) => {
                current_identity_keys.contains(identity.identity_key.as_str())
            }
            ParsedWatchIdentity::LegacyStructured(identity) => current_comments
                .iter()
                .any(|current| identity.matches_comment(current)),
            ParsedWatchIdentity::LegacyUnstructured => true,
        };

        if !comment_still_exists {
            mark_task_done_from_removed_comment(task, now);
            closed.push(task.id.clone());
        }
    }

    closed
}

#[cfg(test)]
pub(crate) fn task_exists_for_comment(queue: &QueueFile, comment: &DetectedComment) -> bool {
    let identity = WatchCommentIdentity::from_detected_comment(comment);
    find_matching_active_watch_task_index(queue, &identity).is_some()
}

pub(super) fn find_matching_active_watch_task_index(
    queue: &QueueFile,
    identity: &WatchCommentIdentity,
) -> Option<usize> {
    queue.tasks.iter().enumerate().find_map(|(index, task)| {
        if !is_active_watch_task(task) {
            return None;
        }

        match parse_task_watch_identity(task)? {
            ParsedWatchIdentity::V2(existing) if existing.identity_key == identity.identity_key => {
                Some(index)
            }
            ParsedWatchIdentity::LegacyStructured(existing)
                if existing.matches_comment(identity) =>
            {
                Some(index)
            }
            _ => None,
        }
    })
}

fn is_active_watch_task(task: &Task) -> bool {
    task.tags.iter().any(|tag| tag == "watch")
        && task.status != TaskStatus::Done
        && task.status != TaskStatus::Rejected
}

fn mark_task_done_from_removed_comment(task: &mut Task, now: &str) {
    task.status = TaskStatus::Done;
    task.completed_at = Some(now.to_string());
    task.updated_at = Some(now.to_string());
    task.notes.push(format!(
        "[watch] Automatically marked done: originating comment was removed at {}",
        now
    ));
}
