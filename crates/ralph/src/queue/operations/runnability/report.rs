//! Full runnability report assembly and selection.
//!
//! Responsibilities:
//! - Build queue-wide runnability reports and summary counts.
//! - Choose the selected task reported to callers using existing queue semantics.
//! - Provide the task-level convenience wrapper for detailed runnability checks.
//!
//! Does not handle:
//! - Low-level blocker analysis details.
//! - Queue persistence or locking.
//!
//! Invariants/assumptions:
//! - Candidate counting matches `queue next` semantics: Todo plus Draft when enabled.
//! - Prefer-doing selection intentionally wins even if a Doing task is blocked.

use anyhow::Result;

use crate::contracts::{BlockingState, QueueFile, Task, TaskStatus};
use crate::queue::operations::RunnableSelectionOptions;

use super::analysis::analyze_task_runnability;
use super::model::{
    NotRunnableReason, QueueRunnabilityReport, QueueRunnabilitySelection, QueueRunnabilitySummary,
    RUNNABILITY_REPORT_VERSION,
};

/// Build a runnability report with the current time.
pub fn queue_runnability_report(
    active: &QueueFile,
    done: Option<&QueueFile>,
    options: RunnableSelectionOptions,
) -> Result<QueueRunnabilityReport> {
    let now = crate::timeutil::now_utc_rfc3339()?;
    queue_runnability_report_at(&now, active, done, options)
}

/// Build a runnability report with a specific timestamp (deterministic for tests).
pub fn queue_runnability_report_at(
    now_rfc3339: &str,
    active: &QueueFile,
    done: Option<&QueueFile>,
    options: RunnableSelectionOptions,
) -> Result<QueueRunnabilityReport> {
    let now_dt = crate::timeutil::parse_rfc3339(now_rfc3339)?;
    let tasks = active
        .tasks
        .iter()
        .map(|task| analyze_task_runnability(task, active, done, now_rfc3339, now_dt, options))
        .collect::<Vec<_>>();

    let mut summary = summarize_rows(active.tasks.len(), &tasks, options);
    summary.blocking =
        derive_queue_blocking_state(&tasks, &summary, options.include_draft, now_rfc3339);
    let selection = build_selection(active, &tasks, options);

    Ok(QueueRunnabilityReport {
        version: RUNNABILITY_REPORT_VERSION,
        now: now_rfc3339.to_string(),
        selection,
        summary,
        tasks,
    })
}

/// Check if a specific task is runnable (convenience wrapper).
pub fn is_task_runnable_detailed(
    task: &Task,
    active: &QueueFile,
    done: Option<&QueueFile>,
    now_rfc3339: &str,
    include_draft: bool,
) -> Result<(bool, Vec<NotRunnableReason>)> {
    let now_dt = crate::timeutil::parse_rfc3339(now_rfc3339)?;
    let options = RunnableSelectionOptions::new(include_draft, false);
    let row = analyze_task_runnability(task, active, done, now_rfc3339, now_dt, options);
    Ok((row.runnable, row.reasons))
}

fn summarize_rows(
    total_active: usize,
    rows: &[super::model::TaskRunnabilityRow],
    options: RunnableSelectionOptions,
) -> QueueRunnabilitySummary {
    let mut candidates_total = 0usize;
    let mut runnable_candidates = 0usize;
    let mut blocked_by_dependencies = 0usize;
    let mut blocked_by_schedule = 0usize;
    let mut blocked_by_status_or_flags = 0usize;

    for row in rows.iter().filter(|row| is_candidate(row.status, options)) {
        candidates_total += 1;
        if row.runnable {
            runnable_candidates += 1;
            continue;
        }

        for reason in &row.reasons {
            match reason {
                NotRunnableReason::StatusNotRunnable { .. } | NotRunnableReason::DraftExcluded => {
                    blocked_by_status_or_flags += 1;
                }
                NotRunnableReason::UnmetDependencies { .. } => blocked_by_dependencies += 1,
                NotRunnableReason::ScheduledStartInFuture { .. } => blocked_by_schedule += 1,
            }
        }
    }

    QueueRunnabilitySummary {
        total_active,
        candidates_total,
        runnable_candidates,
        blocked_by_dependencies,
        blocked_by_schedule,
        blocked_by_status_or_flags,
        blocking: None,
    }
}

fn derive_queue_blocking_state(
    rows: &[super::model::TaskRunnabilityRow],
    summary: &QueueRunnabilitySummary,
    include_draft: bool,
    observed_at: &str,
) -> Option<BlockingState> {
    let stamp = |state: BlockingState| state.with_observed_at(observed_at.to_string());

    if summary.runnable_candidates > 0 {
        return None;
    }

    if summary.candidates_total == 0 {
        return Some(stamp(BlockingState::idle(include_draft)));
    }

    let next_schedule = rows
        .iter()
        .flat_map(|row| row.reasons.iter())
        .filter_map(|reason| match reason {
            NotRunnableReason::ScheduledStartInFuture {
                scheduled_start,
                seconds_until_runnable,
                ..
            } => Some((scheduled_start.clone(), *seconds_until_runnable)),
            _ => None,
        })
        .min_by_key(|(_, seconds)| *seconds);

    match (
        summary.blocked_by_dependencies > 0,
        summary.blocked_by_schedule > 0,
    ) {
        (true, false) => Some(stamp(BlockingState::dependency_blocked(
            summary.blocked_by_dependencies,
        ))),
        (false, true) => Some(stamp(BlockingState::schedule_blocked(
            summary.blocked_by_schedule,
            next_schedule.as_ref().map(|(at, _)| at.clone()),
            next_schedule.as_ref().map(|(_, seconds)| *seconds),
        ))),
        (true, true) => Some(stamp(BlockingState::mixed_queue(
            summary.blocked_by_dependencies,
            summary.blocked_by_schedule,
            summary.blocked_by_status_or_flags,
        ))),
        (false, false) => Some(stamp(BlockingState::idle(include_draft))),
    }
}

fn build_selection(
    active: &QueueFile,
    rows: &[super::model::TaskRunnabilityRow],
    options: RunnableSelectionOptions,
) -> QueueRunnabilitySelection {
    let (selected_task_id, selected_task_status) = if options.prefer_doing
        && let Some(task) = active.tasks.iter().find(|t| t.status == TaskStatus::Doing)
    {
        (Some(task.id.clone()), Some(TaskStatus::Doing))
    } else {
        select_first_runnable_row(rows, options)
            .map(|row| (Some(row.id.clone()), Some(row.status)))
            .unwrap_or((None, None))
    };

    QueueRunnabilitySelection {
        include_draft: options.include_draft,
        prefer_doing: options.prefer_doing,
        selected_task_id,
        selected_task_status,
    }
}

fn select_first_runnable_row(
    rows: &[super::model::TaskRunnabilityRow],
    options: RunnableSelectionOptions,
) -> Option<&super::model::TaskRunnabilityRow> {
    rows.iter().find(|row| {
        row.runnable
            && (row.status == TaskStatus::Todo
                || (options.include_draft && row.status == TaskStatus::Draft))
    })
}

fn is_candidate(status: TaskStatus, options: RunnableSelectionOptions) -> bool {
    status == TaskStatus::Todo || (options.include_draft && status == TaskStatus::Draft)
}
