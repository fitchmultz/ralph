//! Collection/mutation helpers for queue tasks.

use crate::contracts::{QueueFile, TaskStatus};
use std::collections::HashSet;

/// Suggests the insertion index for new tasks based on the first task's status.
///
/// Returns `1` if the first task has status `Doing` (insert after the in-progress task),
/// otherwise returns `0` (insert at top of the queue). Returns `0` for empty queues.
pub fn suggest_new_task_insert_index(queue: &QueueFile) -> usize {
    match queue.tasks.first() {
        Some(first_task) if matches!(first_task.status, TaskStatus::Doing) => 1,
        _ => 0,
    }
}

/// Repositions newly added tasks to the specified insertion index in the queue.
///
/// This function extracts tasks identified by `new_task_ids` from their current positions
/// and splices them into the queue at `insert_at`, preserving the relative order of
/// existing tasks and the new tasks themselves.
///
/// The `insert_at` index is clamped to `queue.tasks.len()` to prevent out-of-bounds errors.
pub fn reposition_new_tasks(queue: &mut QueueFile, new_task_ids: &[String], insert_at: usize) {
    if new_task_ids.is_empty() || queue.tasks.is_empty() {
        return;
    }

    let insert_at = insert_at.min(queue.tasks.len());
    let new_task_set: HashSet<String> = new_task_ids.iter().cloned().collect();

    let mut new_tasks = Vec::new();
    let mut retained_tasks = Vec::new();

    for task in queue.tasks.drain(..) {
        if new_task_set.contains(&task.id) {
            new_tasks.push(task);
        } else {
            retained_tasks.push(task);
        }
    }

    // Splice new tasks at the calculated insertion point
    let split_index = insert_at.min(retained_tasks.len());
    let mut before_split = Vec::new();
    let mut after_split = retained_tasks;
    for task in after_split.drain(..split_index) {
        before_split.push(task);
    }

    queue.tasks = before_split
        .into_iter()
        .chain(new_tasks)
        .chain(after_split)
        .collect();
}

pub fn added_tasks(before: &HashSet<String>, after: &QueueFile) -> Vec<(String, String)> {
    let mut added = Vec::new();
    for task in &after.tasks {
        let id = task.id.trim();
        if id.is_empty() || before.contains(id) {
            continue;
        }
        added.push((id.to_string(), task.title.trim().to_string()));
    }
    added
}

pub fn backfill_missing_fields(
    queue: &mut QueueFile,
    new_task_ids: &[String],
    default_request: &str,
    now_utc: &str,
) {
    let now = now_utc.trim();
    if now.is_empty() || new_task_ids.is_empty() || queue.tasks.is_empty() {
        return;
    }

    let new_task_set: HashSet<&str> = new_task_ids.iter().map(|id| id.as_str()).collect();
    for task in queue.tasks.iter_mut() {
        if !new_task_set.contains(task.id.trim()) {
            continue;
        }

        if task.request.as_ref().is_none_or(|r| r.trim().is_empty()) {
            let req = default_request.trim();
            if !req.is_empty() {
                task.request = Some(req.to_string());
            }
        }

        if task.created_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
            task.created_at = Some(now.to_string());
        }

        if task.updated_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
            task.updated_at = Some(now.to_string());
        }
    }
}

/// Ensure terminal tasks have a completed_at timestamp.
///
/// Returns the number of tasks updated.
pub fn backfill_terminal_completed_at(queue: &mut QueueFile, now_utc: &str) -> usize {
    let now = now_utc.trim();
    if now.is_empty() {
        return 0;
    }

    let mut updated = 0;
    for task in queue.tasks.iter_mut() {
        if !matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
            continue;
        }

        if task
            .completed_at
            .as_ref()
            .is_none_or(|t| t.trim().is_empty())
        {
            task.completed_at = Some(now.to_string());
            updated += 1;
        }
    }

    updated
}

pub fn sort_tasks_by_priority(queue: &mut QueueFile, descending: bool) {
    queue.tasks.sort_by(|a, b| {
        let ord = if descending {
            a.priority.cmp(&b.priority).reverse()
        } else {
            a.priority.cmp(&b.priority)
        };
        match ord {
            std::cmp::Ordering::Equal => a.id.cmp(&b.id),
            other => other,
        }
    });
}

pub fn task_id_set(queue: &QueueFile) -> HashSet<String> {
    let mut set = HashSet::new();
    for task in &queue.tasks {
        let id = task.id.trim();
        if id.is_empty() {
            continue;
        }
        set.insert(id.to_string());
    }
    set
}
