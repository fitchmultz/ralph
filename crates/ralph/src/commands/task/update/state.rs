//! Queue-state preparation, backup, and restore helpers for task updates.
//!
//! Purpose:
//! - Queue-state preparation, backup, and restore helpers for task updates.
//!
//! Responsibilities:
//! - Create pre-update queue backups and restore them when post-run queue handling fails.
//! - Validate queue/done state before and after task-update runner execution.
//! - Capture the pre-update task snapshot needed for change reporting.
//!
//! Not handled here:
//! - Prompt rendering or runner execution.
//! - CLI-facing dry-run output.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Backup restoration targets the active queue file only.
//! - Post-run validation may read `done.jsonc` but must restore only `queue_path` on failure.
//! - Returned task IDs are trimmed before later lookup and reporting.

use crate::contracts::{QueueFile, Task};
use crate::{config, fsutil, queue};
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

pub(super) struct PreparedTaskUpdate {
    pub(super) task_id: String,
    pub(super) before_json: String,
    pub(super) max_depth: u8,
}

pub(super) fn backup_queue_for_update(resolved: &config::Resolved) -> Result<PathBuf> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let backup_path = queue::backup_queue(&resolved.queue_path, &cache_dir)
        .with_context(|| "failed to create queue backup before task update")?;
    log::debug!("Created queue backup at: {}", backup_path.display());
    Ok(backup_path)
}

pub(super) fn prepare_task_update(
    resolved: &config::Resolved,
    task_id: &str,
) -> Result<PreparedTaskUpdate> {
    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    let task_id = task_id.trim();
    let before_task = find_task(&before, task_id)
        .ok_or_else(|| anyhow!(crate::error_messages::task_not_found(task_id)))?;
    let before_json = serde_json::to_string(before_task)?;

    let done = load_done_queue(resolved)?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &before,
        queue::optional_done_queue(&done, &resolved.done_path),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set before task update")?;

    Ok(PreparedTaskUpdate {
        task_id: task_id.to_string(),
        before_json,
        max_depth,
    })
}

pub(super) fn load_done_queue(resolved: &config::Resolved) -> Result<QueueFile> {
    queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))
}

pub(super) fn find_task<'a>(queue_file: &'a QueueFile, task_id: &str) -> Option<&'a Task> {
    queue_file
        .tasks
        .iter()
        .find(|task| task.id.trim() == task_id)
}

pub(super) fn restore_queue_from_backup(queue_path: &Path, backup_path: &Path) -> Result<()> {
    let bytes = std::fs::read(backup_path)
        .with_context(|| format!("read queue backup {}", backup_path.display()))?;
    fsutil::write_atomic(queue_path, &bytes)
        .with_context(|| format!("restore queue from backup {}", backup_path.display()))?;
    Ok(())
}

fn restore_on_failure<T>(
    queue_path: &Path,
    backup_path: &Path,
    action: &str,
    result: Result<T>,
) -> Result<T> {
    result.or_else(
        |err| match restore_queue_from_backup(queue_path, backup_path) {
            Ok(()) => Err(err).with_context(|| {
                format!(
                    "{action}; restored queue from backup {}",
                    backup_path.display()
                )
            }),
            Err(restore_err) => Err(err).with_context(|| {
                format!(
                    "{action} AND restore failed (backup {}): {:#}",
                    backup_path.display(),
                    restore_err
                )
            }),
        },
    )
}

pub(super) fn load_validate_and_save_queue_after_update(
    resolved: &config::Resolved,
    backup_path: &Path,
    max_depth: u8,
) -> Result<QueueFile> {
    let after = restore_on_failure(
        &resolved.queue_path,
        backup_path,
        "queue parse failed after task update",
        queue::load_queue_with_repair(&resolved.queue_path)
            .with_context(|| "parse queue after task update"),
    )?;

    let done_after = load_done_queue(resolved)?;
    restore_on_failure(
        &resolved.queue_path,
        backup_path,
        "queue validation failed after task update",
        queue::validate_queue_set(
            &after,
            queue::optional_done_queue(&done_after, &resolved.done_path),
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )
        .context("validate queue set after task update"),
    )?;

    restore_on_failure(
        &resolved.queue_path,
        backup_path,
        "queue save failed after task update",
        queue::save_queue(&resolved.queue_path, &after).context("save queue after task update"),
    )?;

    Ok(after)
}
