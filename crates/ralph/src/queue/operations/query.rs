//! Query helpers for queue tasks.

use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::{anyhow, bail, Result};

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
    if options.prefer_doing {
        if let Some(idx) = active
            .tasks
            .iter()
            .position(|task| task.status == TaskStatus::Doing)
        {
            return Some(idx);
        }
    }

    if let Some(idx) = active.tasks.iter().position(|task| {
        task.status == TaskStatus::Todo && are_dependencies_met(task, active, done)
    }) {
        return Some(idx);
    }

    if options.include_draft {
        return active.tasks.iter().position(|task| {
            task.status == TaskStatus::Draft && are_dependencies_met(task, active, done)
        });
    }

    None
}

/// Select a runnable task index by target task id, with validation.
pub fn select_runnable_task_index_with_target(
    active: &QueueFile,
    done: Option<&QueueFile>,
    target_task_id: &str,
    options: RunnableSelectionOptions,
) -> Result<usize> {
    let needle = target_task_id.trim();
    if needle.is_empty() {
        bail!("Target task id is empty");
    }
    let idx = active
        .tasks
        .iter()
        .position(|task| task.id.trim() == needle)
        .ok_or_else(|| anyhow!("Target task {} not found in queue", needle))?;
    let task = &active.tasks[idx];
    match task.status {
        TaskStatus::Done | TaskStatus::Rejected => {
            bail!(
                "Target task {} is not runnable (status: {}). Choose a todo/doing task.",
                needle,
                task.status
            );
        }
        TaskStatus::Draft => {
            if !options.include_draft {
                bail!(
                    "Target task {} is in draft status. Use --include-draft to run draft tasks.",
                    needle
                );
            }
            if !are_dependencies_met(task, active, done) {
                bail!(
                    "Target task {} is blocked by unmet dependencies. Resolve dependencies before running.",
                    needle
                );
            }
        }
        TaskStatus::Todo => {
            if !are_dependencies_met(task, active, done) {
                bail!(
                    "Target task {} is blocked by unmet dependencies. Resolve dependencies before running.",
                    needle
                );
            }
        }
        TaskStatus::Doing => {}
    }

    Ok(idx)
}
