//! Task selection helpers for parallel workers.
//!
//! Responsibilities:
//! - Select the next runnable task while the queue lock is held.
//! - Compute the exclusion set for in-flight, attempted, and blocked workers.
//!
//! Does not handle:
//! - Worker subprocess creation or shutdown.
//! - Persisting parallel worker state.

use crate::commands::run::parallel::state;
use crate::commands::run::selection::select_run_one_task_index_excluding;
use crate::config;
use crate::lock::DirLock;
use crate::queue;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

use super::process::WorkerState;

pub(crate) fn select_next_task_locked(
    resolved: &config::Resolved,
    include_draft: bool,
    excluded_ids: &HashSet<String>,
    _queue_lock: &DirLock,
) -> Result<Option<(String, String)>> {
    let (queue_file, done_file) = queue::load_and_validate_queues(resolved, true).map_err(|err| {
        err.context(
            "Parallel worker selection is read-only; run `ralph queue repair --dry-run` and then `ralph queue repair` to apply undo-backed normalization before retrying",
        )
    })?;
    let done_ref = done_file.as_ref();

    let idx =
        select_run_one_task_index_excluding(&queue_file, done_ref, include_draft, excluded_ids)?;
    let idx = match idx {
        Some(idx) => idx,
        None => return Ok(None),
    };
    let task = &queue_file.tasks[idx];
    Ok(Some((
        task.id.trim().to_string(),
        task.title.trim().to_string(),
    )))
}

pub(crate) fn collect_excluded_ids(
    state_file: &state::ParallelStateFile,
    in_flight: &HashMap<String, WorkerState>,
    attempted_in_run: &HashSet<String>,
) -> HashSet<String> {
    let mut excluded = HashSet::new();

    for key in in_flight.keys() {
        excluded.insert(key.trim().to_string());
    }
    for task_id in attempted_in_run {
        excluded.insert(task_id.trim().to_string());
    }
    for worker in &state_file.workers {
        if worker.lifecycle == state::WorkerLifecycle::BlockedPush {
            excluded.insert(worker.task_id.trim().to_string());
        }
    }

    excluded
}
