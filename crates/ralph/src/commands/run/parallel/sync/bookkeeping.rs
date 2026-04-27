//! Coordinator bookkeeping reconciliation for parallel workers.
//!
//! Purpose:
//! - Reconcile successful parallel-worker queue/done results back into the
//!   coordinator checkout when Git cannot carry queue bookkeeping files.
//!
//! Responsibilities:
//! - Decide whether queue/done bookkeeping is Git-authoritative or
//!   coordinator-authoritative.
//! - Merge only a successful worker's assigned terminal task into the current
//!   coordinator queue/done state for untracked or ignored bookkeeping files.
//! - Validate, snapshot, and atomically persist coordinator queue/done updates.
//!
//! Non-scope:
//! - Worker workspace seeding.
//! - Generic ignored-file synchronization beyond queue/done bookkeeping.
//! - Accepting rejected tasks from successful parallel workers.
//!
//! Usage:
//! - Orchestration calls `reconcile_successful_workers` after a successful
//!   worker exits and after tracked branch refresh succeeds.
//!
//! Invariants/Assumptions:
//! - The caller holds the coordinator queue lock for the full reconciliation.
//! - Tracked queue/done files are updated through Git, not by this module.
//! - Untracked or ignored worker queue/done files are result snapshots; only
//!   the assigned task's done record is authoritative.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::commands::run::parallel::path_map::map_resolved_path_into_workspace;
use crate::config;
use crate::contracts::{Task, TaskStatus};
use crate::{git, queue, undo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BookkeepingAuthority {
    GitTracked,
    CoordinatorReconcile,
}

#[derive(Debug, Clone)]
pub(crate) struct SuccessfulWorkerBookkeeping {
    pub(crate) task_id: String,
    pub(crate) workspace_path: PathBuf,
}

pub(crate) fn bookkeeping_authority(resolved: &config::Resolved) -> Result<BookkeepingAuthority> {
    let queue_tracked = is_resolved_path_tracked(resolved, &resolved.queue_path, "queue")?;
    let done_tracked = is_resolved_path_tracked(resolved, &resolved.done_path, "done")?;

    match (queue_tracked, done_tracked) {
        (true, true) => Ok(BookkeepingAuthority::GitTracked),
        (false, false) => Ok(BookkeepingAuthority::CoordinatorReconcile),
        (queue_tracked, done_tracked) => bail!(
            "parallel bookkeeping cannot safely reconcile split queue/done tracking state: queue tracked={}, done tracked={}. Track both queue and done or ignore/untrack both.",
            queue_tracked,
            done_tracked
        ),
    }
}

pub(crate) fn reconcile_successful_workers(
    resolved: &config::Resolved,
    _queue_lock: &crate::lock::DirLock,
    workers: &[SuccessfulWorkerBookkeeping],
    operation: &str,
) -> Result<BookkeepingAuthority> {
    let authority = bookkeeping_authority(resolved)?;
    if authority == BookkeepingAuthority::GitTracked || workers.is_empty() {
        return Ok(authority);
    }

    let mut active = queue::load_queue(&resolved.queue_path).with_context(|| {
        format!(
            "load coordinator queue {} for parallel bookkeeping reconciliation",
            resolved.queue_path.display()
        )
    })?;
    let mut done = queue::load_queue_or_default(&resolved.done_path).with_context(|| {
        format!(
            "load coordinator done {} for parallel bookkeeping reconciliation",
            resolved.done_path.display()
        )
    })?;

    for worker in workers {
        let terminal = worker_done_task(resolved, &worker.workspace_path, &worker.task_id)?;
        let task_id = terminal.id.trim().to_string();
        active.tasks.retain(|task| task.id.trim() != task_id);
        done.tasks.retain(|task| task.id.trim() != task_id);
        done.tasks.push(terminal);
    }

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &active,
        Some(&done),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate parallel coordinator queue/done reconciliation")?;

    undo::create_undo_snapshot(resolved, operation)
        .context("create undo snapshot before parallel queue/done reconciliation")?;

    queue::save_queue(&resolved.done_path, &done).with_context(|| {
        format!(
            "save coordinator done {} after parallel bookkeeping reconciliation",
            resolved.done_path.display()
        )
    })?;
    queue::save_queue(&resolved.queue_path, &active).with_context(|| {
        format!(
            "save coordinator queue {} after parallel bookkeeping reconciliation",
            resolved.queue_path.display()
        )
    })?;

    log::info!(
        "Reconciled {} successful parallel worker bookkeeping result(s) into coordinator queue/done",
        workers.len()
    );
    Ok(authority)
}

fn is_resolved_path_tracked(resolved: &config::Resolved, path: &Path, label: &str) -> Result<bool> {
    let rel = path.strip_prefix(&resolved.repo_root).with_context(|| {
        format!(
            "{} bookkeeping path {} is not under repo root {}",
            label,
            path.display(),
            resolved.repo_root.display()
        )
    })?;
    let rel = rel.to_string_lossy().to_string();
    git::is_path_tracked(&resolved.repo_root, &rel)
        .with_context(|| format!("check whether {} bookkeeping path is tracked", label))
}

fn worker_done_task(
    resolved: &config::Resolved,
    workspace_path: &Path,
    task_id: &str,
) -> Result<Task> {
    let worker_done_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        workspace_path,
        &resolved.done_path,
        "done",
    )
    .context("map done bookkeeping path from worker workspace")?;

    let done = queue::load_queue_or_default(&worker_done_path).with_context(|| {
        format!(
            "load worker done {} for successful task {}",
            worker_done_path.display(),
            task_id
        )
    })?;
    let normalized_task_id = task_id.trim();
    let mut matches = done
        .tasks
        .into_iter()
        .filter(|task| task.id.trim() == normalized_task_id)
        .collect::<Vec<_>>();

    match matches.len() {
        1 => {
            let task = matches.remove(0);
            if task.status != TaskStatus::Done {
                bail!(
                    "successful parallel worker {} produced terminal task with status {}; expected done",
                    normalized_task_id,
                    task.status
                );
            }
            if task.completed_at.as_deref().unwrap_or("").trim().is_empty() {
                bail!(
                    "successful parallel worker {} produced done task without completed_at",
                    normalized_task_id
                );
            }
            Ok(task)
        }
        0 => bail!(
            "successful parallel worker {} did not archive its task in {}",
            normalized_task_id,
            worker_done_path.display()
        ),
        count => bail!(
            "successful parallel worker {} produced {} done entries for the same task",
            normalized_task_id,
            count
        ),
    }
}

#[cfg(test)]
mod tests;
