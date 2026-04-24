//! Watch task orchestration.
//!
//! Purpose:
//! - Watch task orchestration.
//!
//! Responsibilities:
//! - Load/save queue state around watch comment processing.
//! - Coordinate deduplication, metadata upgrades, reconciliation, and notifications.
//!
//! Not handled here:
//! - Comment detection.
//! - Watch identity parsing internals.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue writes occur only when the watch run changes queue state.
//! - Legacy structured tasks upgrade in place when the same comment still exists.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::commands::watch::identity::{
    ParsedWatchIdentity, WatchCommentIdentity, parse_task_watch_identity, upgrade_task_to_v2,
};
use crate::commands::watch::types::{DetectedComment, WatchOptions};
use crate::config::Resolved;
use crate::notification::{
    NotificationOverrides, build_notification_config, notify_watch_new_task,
};
use crate::queue::{load_queue, save_queue, suggest_new_task_insert_index};
use crate::timeutil;

use super::materialize::create_task_from_comment;
use super::reconcile::{find_matching_active_watch_task_index, reconcile_watch_tasks_in_queue};

pub fn handle_detected_comments(
    resolved: &Resolved,
    comments: &[DetectedComment],
    processed_files: &[PathBuf],
    opts: &WatchOptions,
) -> Result<()> {
    let mut queue = load_queue(&resolved.queue_path)
        .with_context(|| format!("load queue {}", resolved.queue_path.display()))?;
    let now = timeutil::now_utc_rfc3339_or_fallback();
    let mut created_tasks = Vec::new();
    let mut queue_changed = false;

    for comment in comments {
        let identity = WatchCommentIdentity::from_detected_comment(comment);

        if let Some(index) = find_matching_active_watch_task_index(&queue, &identity) {
            if matches!(
                parse_task_watch_identity(&queue.tasks[index]),
                Some(ParsedWatchIdentity::LegacyStructured(_))
            ) {
                let task = &mut queue.tasks[index];
                upgrade_task_to_v2(task, &identity);
                task.updated_at = Some(now.clone());
                task.notes.push(format!(
                    "[watch] Automatically upgraded metadata to watch.version=2 at {}",
                    now
                ));
                queue_changed = true;
            }
            continue;
        }

        let task = create_task_from_comment(comment, resolved)?;
        if opts.auto_queue {
            let insert_at = suggest_new_task_insert_index(&queue);
            created_tasks.push((task.id.clone(), task.title.clone()));
            queue.tasks.insert(insert_at, task);
            queue_changed = true;
        } else {
            let type_str = format!("{:?}", comment.comment_type).to_uppercase();
            log::info!(
                "[SUGGESTION] {} at {}:{}",
                type_str,
                comment.file_path.display(),
                comment.line_number
            );
            log::info!("  Content: {}", comment.content);
            log::info!("  Suggested task: {}", task.title);
        }
    }

    if opts.close_removed {
        let closed = reconcile_watch_tasks_in_queue(&mut queue, comments, processed_files, &now);
        if !closed.is_empty() {
            queue_changed = true;
            log::info!(
                "Reconciled {} watch task(s) due to removed comments",
                closed.len()
            );
        }
    }

    if queue_changed {
        save_queue(&resolved.queue_path, &queue)
            .with_context(|| format!("save queue {}", resolved.queue_path.display()))?;
    }

    if opts.auto_queue && !created_tasks.is_empty() {
        log::info!("Added {} task(s) to queue", created_tasks.len());
        if opts.notify {
            let config = build_notification_config(
                &resolved.config.agent.notification,
                &NotificationOverrides::default(),
            );
            notify_watch_new_task(created_tasks.len(), &config);
        }
    }

    Ok(())
}

#[cfg(test)]
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
