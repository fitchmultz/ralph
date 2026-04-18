//! Queue maintenance operations for post-run supervision.
//!
//! Responsibilities:
//! - Explicitly repair and validate queue and done files for post-run recovery.
//! - Ensure task status transitions to Done appropriately.
//! - Handle queue/done file persistence.
//!
//! Not handled here:
//! - Git operations (see git_ops.rs).
//! - CI gate execution (see ci.rs).
//! - Notification logic (see notify.rs).
//!
//! Invariants/assumptions:
//! - Queue files follow the QueueFile schema.
//! - Repair writes are applied through undo-backed queue repair helpers.
//! - Task IDs are unique across queue and done files.

use crate::contracts::{QueueFile, TaskStatus};
use crate::runutil;
use crate::{queue, timeutil};
use anyhow::{Result, anyhow, bail};

/// Applies undo-backed queue repair when needed, then validates queue and done files.
pub(crate) fn maintain_and_validate_queues(
    resolved: &crate::config::Resolved,
    queue_lock: Option<&crate::lock::DirLock>,
) -> Result<(QueueFile, QueueFile)> {
    if let Some(queue_lock) = queue_lock {
        queue::apply_queue_maintenance_repair_with_undo(
            resolved,
            queue_lock,
            "post-run queue maintenance",
        )?;
    } else {
        let queue_lock =
            queue::acquire_queue_lock(&resolved.repo_root, "post-run queue repair", false)?;
        queue::apply_queue_maintenance_repair_with_undo(
            resolved,
            &queue_lock,
            "post-run queue maintenance",
        )?;
    }

    let (queue_file, done_file_opt) = queue::load_and_validate_queues(resolved, true)?;
    let done_file = done_file_opt.unwrap_or_default();

    Ok((queue_file, done_file))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostRunQueueMutationPlan {
    pub task_status: TaskStatus,
    pub task_title: String,
    pub in_done: bool,
    pub mark_task_done: bool,
    pub archive_candidate_ids: Vec<String>,
}

impl PostRunQueueMutationPlan {
    pub(crate) fn will_mutate_queue_files(&self) -> bool {
        self.mark_task_done || !self.archive_candidate_ids.is_empty()
    }

    pub(crate) fn task_already_archived_done(&self) -> bool {
        self.task_status == TaskStatus::Done && self.in_done
    }
}

pub(crate) fn build_post_run_queue_mutation_plan(
    queue_file: &QueueFile,
    done_file: &QueueFile,
    task_id: &str,
) -> Result<PostRunQueueMutationPlan> {
    let (task_status, task_title, in_done) = require_task_status(queue_file, done_file, task_id)?;
    let mark_task_done = task_status != TaskStatus::Done;
    let task_id = task_id.trim();

    let mut archive_candidate_ids = queue_file
        .tasks
        .iter()
        .filter(|task| matches!(task.status, TaskStatus::Done | TaskStatus::Rejected))
        .map(|task| task.id.trim().to_string())
        .collect::<Vec<_>>();

    if mark_task_done && !archive_candidate_ids.iter().any(|id| id == task_id) {
        archive_candidate_ids.push(task_id.to_string());
    }

    Ok(PostRunQueueMutationPlan {
        task_status,
        task_title,
        in_done,
        mark_task_done,
        archive_candidate_ids,
    })
}

/// Returns the status and title of a task, or an error if not found.
pub(crate) fn require_task_status(
    queue_file: &QueueFile,
    done_file: &QueueFile,
    task_id: &str,
) -> Result<(TaskStatus, String, bool)> {
    find_task_status(queue_file, done_file, task_id).ok_or_else(|| {
        anyhow!(
            "{}",
            crate::error_messages::task_not_found_in_queue_or_done(task_id)
        )
    })
}

/// Finds a task's status, title, and whether it's in the done file.
pub(crate) fn find_task_status(
    queue_file: &QueueFile,
    done_file: &QueueFile,
    task_id: &str,
) -> Option<(TaskStatus, String, bool)> {
    let needle = task_id.trim();
    if let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == needle) {
        return Some((task.status, task.title.clone(), false));
    }
    if let Some(task) = done_file.tasks.iter().find(|t| t.id.trim() == needle) {
        return Some((task.status, task.title.clone(), true));
    }
    None
}

/// Ensures a task is marked as Done when the repo is dirty, handling revert-mode on inconsistency.
pub(crate) fn ensure_task_done_dirty_or_revert(
    resolved: &crate::config::Resolved,
    queue_file: &mut QueueFile,
    task_id: &str,
    task_status: TaskStatus,
    in_done: bool,
    git_revert_mode: crate::contracts::GitRevertMode,
    revert_prompt: Option<&runutil::RevertPromptHandler>,
) -> Result<()> {
    if task_status != TaskStatus::Done {
        if in_done {
            let outcome = runutil::apply_git_revert_mode(
                &resolved.repo_root,
                git_revert_mode,
                "Task inconsistency detected",
                revert_prompt,
            )?;
            bail!(
                "{}",
                runutil::format_revert_failure_message(
                    &format!(
                        "Task inconsistency: task {task_id} is archived in .ralph/done.jsonc but its status is not 'done'. Review the task state in .ralph/done.jsonc."
                    ),
                    outcome,
                )
            );
        }
        let now = timeutil::now_utc_rfc3339()?;
        queue::set_status(queue_file, task_id, TaskStatus::Done, &now, None)?;
        queue::save_queue(&resolved.queue_path, queue_file)?;
    }
    Ok(())
}

/// Ensures a task is marked as Done when the repo is clean, bailing on inconsistency.
pub(crate) fn ensure_task_done_clean_or_bail(
    resolved: &crate::config::Resolved,
    queue_file: &mut QueueFile,
    task_id: &str,
    task_status: TaskStatus,
    in_done: bool,
) -> Result<bool> {
    if task_status != TaskStatus::Done {
        if in_done {
            bail!(
                "Task inconsistency: task {task_id} is archived in .ralph/done.jsonc but its status is not 'done'. Review the task state in .ralph/done.jsonc."
            );
        }
        let now = timeutil::now_utc_rfc3339()?;
        queue::set_status(queue_file, task_id, TaskStatus::Done, &now, None)?;
        queue::save_queue(&resolved.queue_path, queue_file)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
#[path = "queue_ops_tests.rs"]
mod tests;
