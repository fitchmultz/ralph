//! Per-task runnability analysis helpers.
//!
//! Purpose:
//! - Per-task runnability analysis helpers.
//!
//! Responsibilities:
//! - Analyze a single task against status, dependency, and schedule blockers.
//! - Build dependency issue details for callers and reports.
//! - Keep blocker ordering and rule evaluation centralized.
//!
//! Non-scope:
//! - Full report aggregation or selection.
//! - Queue persistence or task mutation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Reasons are appended in stable order: status/flags, dependencies, schedule.
//! - Missing dependencies are reported as blocking.

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue::operations::{RunnableSelectionOptions, find_task_across};

use super::model::{DependencyIssue, NotRunnableReason, TaskRunnabilityRow};

pub(super) fn analyze_task_runnability(
    task: &Task,
    active: &QueueFile,
    done: Option<&QueueFile>,
    now_rfc3339: &str,
    now_dt: time::OffsetDateTime,
    options: RunnableSelectionOptions,
) -> TaskRunnabilityRow {
    let mut reasons = Vec::new();
    let mut runnable = true;

    match task.status {
        TaskStatus::Done | TaskStatus::Rejected => {
            runnable = false;
            reasons.push(NotRunnableReason::StatusNotRunnable {
                status: task.status,
            });
        }
        TaskStatus::Draft if !options.include_draft => {
            runnable = false;
            reasons.push(NotRunnableReason::DraftExcluded);
        }
        TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing => {}
    }

    if runnable || reasons.is_empty() {
        let dependency_issues = dependency_issues(task, active, done);
        if !dependency_issues.is_empty() {
            runnable = false;
            reasons.push(NotRunnableReason::UnmetDependencies {
                dependencies: dependency_issues,
            });
        }
    }

    if should_check_schedule(&reasons)
        && let Some(ref scheduled) = task.scheduled_start
        && let Ok(scheduled_dt) = crate::timeutil::parse_rfc3339(scheduled)
        && scheduled_dt > now_dt
    {
        runnable = false;
        reasons.push(NotRunnableReason::ScheduledStartInFuture {
            scheduled_start: scheduled.clone(),
            now: now_rfc3339.to_string(),
            seconds_until_runnable: (scheduled_dt - now_dt).whole_seconds(),
        });
    }

    TaskRunnabilityRow {
        id: task.id.clone(),
        status: task.status,
        runnable,
        reasons,
    }
}

fn should_check_schedule(reasons: &[NotRunnableReason]) -> bool {
    reasons
        .iter()
        .all(|reason| !matches!(reason, NotRunnableReason::StatusNotRunnable { .. }))
}

fn dependency_issues(
    task: &Task,
    active: &QueueFile,
    done: Option<&QueueFile>,
) -> Vec<DependencyIssue> {
    task.depends_on
        .iter()
        .filter_map(|dep_id| match find_task_across(active, done, dep_id) {
            Some(dep_task)
                if dep_task.status == TaskStatus::Done
                    || dep_task.status == TaskStatus::Rejected =>
            {
                None
            }
            Some(dep_task) => Some(DependencyIssue::NotComplete {
                id: dep_id.clone(),
                status: dep_task.status,
            }),
            None => Some(DependencyIssue::Missing { id: dep_id.clone() }),
        })
        .collect()
}
