//! Collection/mutation helpers for queue tasks.

use crate::contracts::QueueFile;
use std::collections::HashSet;

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
    if now.is_empty() {
        return;
    }

    for task in queue.tasks.iter_mut() {
        if !new_task_ids.contains(&task.id.trim().to_string()) {
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
