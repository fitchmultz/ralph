//! Query helpers for queue tasks.
//!
//! Purpose:
//! - Query helpers for queue tasks.
//!
//! Responsibilities:
//! - Locate tasks in active/done queues and determine runnable indices.
//! - Enforce runnable status and dependency rules for selection.
//! - Emit typed `QueueQueryError` for stable test assertions.
//!
//! Non-scope:
//! - Persisting queue data or mutating task fields.
//! - Normalizing IDs beyond trimming whitespace.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Queues are already loaded and represent the source of truth.
//! - Task IDs are matched after trimming and are case-sensitive.
//! - Query errors wrap typed `QueueQueryError` variants for downcasting in tests.

use super::QueueQueryError;
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::timeutil;
use anyhow::Result;

pub fn find_task<'a>(queue: &'a QueueFile, task_id: &str) -> Option<&'a Task> {
    let needle = task_id.trim();
    if needle.is_empty() {
        return None;
    }
    queue.tasks.iter().find(|task| task.id.trim() == needle)
}

pub fn find_task_across<'a>(
    active: &'a QueueFile,
    done: Option<&'a QueueFile>,
    task_id: &str,
) -> Option<&'a Task> {
    find_task(active, task_id).or_else(|| done.and_then(|d| find_task(d, task_id)))
}

#[derive(Clone, Copy, Debug)]
pub struct RunnableSelectionOptions {
    pub include_draft: bool,
    pub prefer_doing: bool,
}

impl RunnableSelectionOptions {
    pub fn new(include_draft: bool, prefer_doing: bool) -> Self {
        Self {
            include_draft,
            prefer_doing,
        }
    }
}

/// Return the first todo task by file order (top-of-file wins).
pub fn next_todo_task(queue: &QueueFile) -> Option<&Task> {
    queue
        .tasks
        .iter()
        .find(|task| task.status == TaskStatus::Todo)
}

/// Check if a task's dependencies are met.
///
/// Dependencies are met if `depends_on` is empty OR all referenced tasks exist and have `status == TaskStatus::Done` or `TaskStatus::Rejected`.
pub fn are_dependencies_met(task: &Task, active: &QueueFile, done: Option<&QueueFile>) -> bool {
    if task.depends_on.is_empty() {
        return true;
    }

    for dep_id in &task.depends_on {
        let dep_task = find_task_across(active, done, dep_id);
        match dep_task {
            Some(t) => {
                if t.status != TaskStatus::Done && t.status != TaskStatus::Rejected {
                    return false;
                }
            }
            None => return false, // Dependency not found means not met
        }
    }

    true
}

/// Check if a task's scheduled_start is in the future.
///
/// Returns true if the task has a scheduled_start timestamp that is
/// in the future relative to the current time.
pub fn is_task_scheduled_for_future(task: &Task) -> bool {
    if let Some(ref scheduled) = task.scheduled_start
        && let Ok(scheduled_dt) = timeutil::parse_rfc3339(scheduled)
        && let Ok(now) = timeutil::now_utc_rfc3339()
        && let Ok(now_dt) = timeutil::parse_rfc3339(&now)
    {
        return scheduled_dt > now_dt;
    }
    false
}

/// Check if a task is runnable (dependencies met and scheduling satisfied).
///
/// A task is runnable if:
/// - All dependencies are met (depends_on tasks are Done or Rejected)
/// - The scheduled_start time has passed (or is not set)
pub fn is_task_runnable(task: &Task, active: &QueueFile, done: Option<&QueueFile>) -> bool {
    are_dependencies_met(task, active, done) && !is_task_scheduled_for_future(task)
}

/// Return the first runnable task (Todo and dependencies met).
pub fn next_runnable_task<'a>(
    active: &'a QueueFile,
    done: Option<&'a QueueFile>,
) -> Option<&'a Task> {
    select_runnable_task_index(active, done, RunnableSelectionOptions::new(false, false))
        .and_then(|idx| active.tasks.get(idx))
}

/// Select the next runnable task index according to the provided options.
///
/// Order:
/// - If `prefer_doing` is true, prefer the first `Doing` task.
/// - Otherwise, choose the first runnable `Todo`.
/// - If `include_draft` is true and no runnable `Todo` exists, choose the first runnable `Draft`.
pub fn select_runnable_task_index(
    active: &QueueFile,
    done: Option<&QueueFile>,
    options: RunnableSelectionOptions,
) -> Option<usize> {
    if options.prefer_doing
        && let Some(idx) = active
            .tasks
            .iter()
            .position(|task| task.status == TaskStatus::Doing)
    {
        return Some(idx);
    }

    if let Some(idx) = active
        .tasks
        .iter()
        .position(|task| task.status == TaskStatus::Todo && is_task_runnable(task, active, done))
    {
        return Some(idx);
    }

    if options.include_draft {
        return active.tasks.iter().position(|task| {
            task.status == TaskStatus::Draft && is_task_runnable(task, active, done)
        });
    }

    None
}

/// Select a runnable task index by target task id, with validation.
pub fn select_runnable_task_index_with_target(
    active: &QueueFile,
    done: Option<&QueueFile>,
    target_task_id: &str,
    operation: &str,
    options: RunnableSelectionOptions,
) -> Result<usize> {
    let needle = target_task_id.trim();
    if needle.is_empty() {
        return Err(QueueQueryError::MissingTargetTaskId {
            operation: operation.to_string(),
        }
        .into());
    }
    let idx = active
        .tasks
        .iter()
        .position(|task| task.id.trim() == needle)
        .ok_or_else(|| QueueQueryError::TargetTaskNotFound {
            operation: operation.to_string(),
            task_id: needle.to_string(),
        })?;
    let task = &active.tasks[idx];
    match task.status {
        TaskStatus::Done | TaskStatus::Rejected => {
            return Err(QueueQueryError::TargetTaskNotRunnable {
                operation: operation.to_string(),
                task_id: needle.to_string(),
                status: task.status,
            }
            .into());
        }
        TaskStatus::Draft => {
            if !options.include_draft {
                return Err(QueueQueryError::TargetTaskDraftExcluded {
                    operation: operation.to_string(),
                    task_id: needle.to_string(),
                }
                .into());
            }
            if !are_dependencies_met(task, active, done) {
                return Err(QueueQueryError::TargetTaskBlockedByUnmetDependencies {
                    operation: operation.to_string(),
                    task_id: needle.to_string(),
                }
                .into());
            }
            if is_task_scheduled_for_future(task) {
                return Err(QueueQueryError::TargetTaskScheduledForFuture {
                    operation: operation.to_string(),
                    task_id: needle.to_string(),
                    scheduled_start: task
                        .scheduled_start
                        .as_deref()
                        .unwrap_or("unknown")
                        .to_string(),
                }
                .into());
            }
        }
        TaskStatus::Todo => {
            if !are_dependencies_met(task, active, done) {
                return Err(QueueQueryError::TargetTaskBlockedByUnmetDependencies {
                    operation: operation.to_string(),
                    task_id: needle.to_string(),
                }
                .into());
            }
            if is_task_scheduled_for_future(task) {
                return Err(QueueQueryError::TargetTaskScheduledForFuture {
                    operation: operation.to_string(),
                    task_id: needle.to_string(),
                    scheduled_start: task
                        .scheduled_start
                        .as_deref()
                        .unwrap_or("unknown")
                        .to_string(),
                }
                .into());
            }
        }
        TaskStatus::Doing => {}
    }

    Ok(idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use std::collections::HashMap;
    use time::OffsetDateTime;

    fn make_task(id: &str, status: TaskStatus, scheduled_start: Option<&str>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: format!("Task {}", id),
            description: None,
            priority: Default::default(),
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: scheduled_start.map(|s| s.to_string()),
            estimated_minutes: None,
            actual_minutes: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn test_is_task_scheduled_for_future_with_future_date() {
        let future = (OffsetDateTime::now_utc() + time::Duration::hours(24))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let task = make_task("RQ-0001", TaskStatus::Todo, Some(&future));
        assert!(is_task_scheduled_for_future(&task));
    }

    #[test]
    fn test_is_task_scheduled_for_future_with_past_date() {
        let past = (OffsetDateTime::now_utc() - time::Duration::hours(24))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let task = make_task("RQ-0001", TaskStatus::Todo, Some(&past));
        assert!(!is_task_scheduled_for_future(&task));
    }

    #[test]
    fn test_is_task_scheduled_for_future_with_no_schedule() {
        let task = make_task("RQ-0001", TaskStatus::Todo, None);
        assert!(!is_task_scheduled_for_future(&task));
    }

    #[test]
    fn test_is_task_runnable_with_schedule_and_dependencies() {
        let past = (OffsetDateTime::now_utc() - time::Duration::hours(24))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let task = make_task("RQ-0001", TaskStatus::Todo, Some(&past));
        let active = QueueFile {
            version: 1,
            tasks: vec![task.clone()],
        };
        assert!(is_task_runnable(&task, &active, None));
    }

    #[test]
    fn test_is_task_not_runnable_with_future_schedule() {
        let future = (OffsetDateTime::now_utc() + time::Duration::hours(24))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let task = make_task("RQ-0001", TaskStatus::Todo, Some(&future));
        let active = QueueFile {
            version: 1,
            tasks: vec![task.clone()],
        };
        assert!(!is_task_runnable(&task, &active, None));
    }

    #[test]
    fn test_select_runnable_task_index_skips_future_scheduled() {
        let future = (OffsetDateTime::now_utc() + time::Duration::hours(24))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let past = (OffsetDateTime::now_utc() - time::Duration::hours(24))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();

        let tasks = vec![
            make_task("RQ-0001", TaskStatus::Todo, Some(&future)), // scheduled future
            make_task("RQ-0002", TaskStatus::Todo, Some(&past)),   // scheduled past (runnable)
        ];
        let active = QueueFile { version: 1, tasks };

        // Should select RQ-0002 (index 1) since RQ-0001 is scheduled for future
        let idx =
            select_runnable_task_index(&active, None, RunnableSelectionOptions::new(false, false));
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn test_select_runnable_task_index_all_future_scheduled() {
        let future = (OffsetDateTime::now_utc() + time::Duration::hours(24))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();

        let tasks = vec![
            make_task("RQ-0001", TaskStatus::Todo, Some(&future)),
            make_task("RQ-0002", TaskStatus::Todo, Some(&future)),
        ];
        let active = QueueFile { version: 1, tasks };

        // No runnable tasks
        let idx =
            select_runnable_task_index(&active, None, RunnableSelectionOptions::new(false, false));
        assert_eq!(idx, None);
    }
}
