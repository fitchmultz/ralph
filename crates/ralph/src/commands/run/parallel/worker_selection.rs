//! Task selection helpers for parallel workers.
//!
//! Purpose:
//! - Task selection helpers for parallel workers.
//!
//! Responsibilities:
//! - Select the next runnable task while the queue lock is held.
//! - Compute the exclusion set for in-flight, attempted, and blocked workers.
//!
//! Non-scope:
//! - Worker subprocess creation or shutdown.
//! - Persisting parallel worker state.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::commands::run::parallel::state;
use crate::commands::run::selection::select_run_one_task_index_excluding;
use crate::config;
use crate::contracts::{BlockingState, Task, TaskStatus};
use crate::lock::DirLock;
use crate::queue;
use crate::queue::operations::{QueueRunnabilitySummary, RunnableSelectionOptions};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

use super::process::WorkerState;

const READ_ONLY_SELECTION_REPAIR_HINT: &str = "Parallel worker selection is read-only; run `ralph queue repair --dry-run` and then `ralph queue repair` to apply undo-backed normalization before retrying";

pub(crate) enum NextTaskSelection {
    Runnable((String, String)),
    NoCandidates {
        blocking: Box<BlockingState>,
    },
    Blocked {
        summary: Box<QueueRunnabilitySummary>,
        blocking: Box<BlockingState>,
    },
}

pub(crate) fn select_next_task_locked(
    resolved: &config::Resolved,
    include_draft: bool,
    excluded_ids: &HashSet<String>,
    _queue_lock: &DirLock,
) -> Result<Option<(String, String)>> {
    match select_next_task_state_locked(resolved, include_draft, excluded_ids, _queue_lock)? {
        NextTaskSelection::Runnable(task) => Ok(Some(task)),
        NextTaskSelection::NoCandidates { .. } | NextTaskSelection::Blocked { .. } => Ok(None),
    }
}

pub(crate) fn select_next_task_state_locked(
    resolved: &config::Resolved,
    include_draft: bool,
    excluded_ids: &HashSet<String>,
    _queue_lock: &DirLock,
) -> Result<NextTaskSelection> {
    let (queue_file, done_file) =
        queue::load_and_validate_queues_without_warning_logs(resolved, true)
            .map_err(|err| err.context(READ_ONLY_SELECTION_REPAIR_HINT))?;
    let done_ref = done_file.as_ref();

    let idx =
        select_run_one_task_index_excluding(&queue_file, done_ref, include_draft, excluded_ids)?;
    let idx = match idx {
        Some(idx) => idx,
        None => {
            let candidates = candidate_tasks(&queue_file.tasks, include_draft, excluded_ids);
            if candidates.is_empty() {
                return Ok(NextTaskSelection::NoCandidates {
                    blocking: Box::new(
                        BlockingState::idle(include_draft)
                            .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback()),
                    ),
                });
            }

            let summary = build_blocked_summary(&queue_file, done_ref, &candidates, include_draft);
            let blocking = summary.blocking.clone().unwrap_or_else(|| {
                BlockingState::idle(include_draft)
                    .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback())
            });
            return Ok(NextTaskSelection::Blocked {
                summary: Box::new(summary),
                blocking: Box::new(blocking),
            });
        }
    };
    let task = &queue_file.tasks[idx];
    Ok(NextTaskSelection::Runnable((
        task.id.trim().to_string(),
        task.title.trim().to_string(),
    )))
}

fn candidate_tasks(
    tasks: &[Task],
    include_draft: bool,
    excluded_ids: &HashSet<String>,
) -> Vec<Task> {
    tasks
        .iter()
        .filter(|task| {
            (task.status == TaskStatus::Todo || (include_draft && task.status == TaskStatus::Draft))
                && !excluded_ids.contains(task.id.trim())
        })
        .cloned()
        .collect()
}

fn build_blocked_summary(
    queue_file: &crate::contracts::QueueFile,
    done_ref: Option<&crate::contracts::QueueFile>,
    candidates: &[Task],
    include_draft: bool,
) -> QueueRunnabilitySummary {
    let options = RunnableSelectionOptions::new(include_draft, true);
    match crate::queue::operations::queue_runnability_report(queue_file, done_ref, options) {
        Ok(report) => report.summary.clone(),
        Err(_) => QueueRunnabilitySummary {
            total_active: queue_file.tasks.len(),
            candidates_total: candidates.len(),
            runnable_candidates: 0,
            blocked_by_dependencies: candidates.len(),
            blocked_by_schedule: 0,
            blocked_by_status_or_flags: 0,
            blocking: Some(
                BlockingState::dependency_blocked(candidates.len())
                    .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback()),
            ),
        },
    }
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
