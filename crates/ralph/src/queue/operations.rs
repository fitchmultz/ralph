//! Task queue task-level operations.
//!
//! This module contains operations that mutate or query tasks within queue files,
//! such as completing tasks, setting statuses/fields, finding tasks, deleting tasks,
//! and sorting tasks by priority. Persistence helpers (load/save/locks/repair) live
//! in `crate::queue` and are called from here when needed.

use super::{load_queue, load_queue_or_default, save_queue, validation};
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::redaction;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct ArchiveReport {
    pub moved_ids: Vec<String>,
}

pub fn archive_done_tasks(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
) -> Result<ArchiveReport> {
    let mut active = load_queue(queue_path)?;
    let mut done = load_queue_or_default(done_path)?;

    validation::validate_queue_set(&active, Some(&done), id_prefix, id_width)?;

    let mut moved_ids = Vec::new();
    let mut remaining = Vec::new();

    for task in active.tasks.into_iter() {
        if task.status != TaskStatus::Done {
            remaining.push(task);
            continue;
        }

        let key = task.id.trim().to_string();
        moved_ids.push(key);
        done.tasks.push(task);
    }

    active.tasks = remaining;

    if moved_ids.is_empty() {
        return Ok(ArchiveReport { moved_ids });
    }

    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(ArchiveReport { moved_ids })
}

/// Complete a single task and move it to the done archive.
///
/// Validates that the task exists in the active queue, is in a valid
/// starting state (todo or doing), updates its status and timestamps,
/// appends any provided notes, and atomically moves it from queue.json
/// to the end of done.json.
///
/// # Arguments
/// * `queue_path` - Path to the active queue file
/// * `done_path` - Path to the done archive file (created if missing)
/// * `task_id` - ID of the task to complete
/// * `status` - Terminal status (Done or Rejected)
/// * `now_rfc3339` - Current UTC timestamp as RFC3339 string
/// * `notes` - Optional notes to append to the task
/// * `id_prefix` - Expected task ID prefix (e.g., "RQ")
/// * `id_width` - Expected numeric width for task IDs (e.g., 4)
#[allow(clippy::too_many_arguments)]
pub fn complete_task(
    queue_path: &Path,
    done_path: &Path,
    task_id: &str,
    status: TaskStatus,
    now_rfc3339: &str,
    notes: &[String],
    id_prefix: &str,
    id_width: usize,
) -> Result<()> {
    // Validate the completion status is terminal
    match status {
        TaskStatus::Done | TaskStatus::Rejected => {
            // Valid terminal statuses
        }
        TaskStatus::Todo | TaskStatus::Doing => {
            bail!(
                "Invalid completion status: only 'done' or 'rejected' are allowed. Got: {:?}. Use 'ralph queue complete {} done' or 'ralph queue complete {} rejected'.",
                status, task_id, task_id
            );
        }
    }

    // Load and validate the active queue
    let mut active = load_queue(queue_path)?;
    validation::validate_queue(&active, id_prefix, id_width)?;

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    // Find the task in the active queue
    let task_idx = active
        .tasks
        .iter()
        .position(|t| t.id.trim() == needle)
        .ok_or_else(|| {
            anyhow!(
                "task not found in active queue: {}. Ensure the task exists in .ralph/queue.json.",
                needle
            )
        })?;

    let task = &active.tasks[task_idx];

    // Validate that the task is in a state that can be completed
    match task.status {
        TaskStatus::Todo | TaskStatus::Doing => {
            // Valid starting states
        }
        TaskStatus::Done | TaskStatus::Rejected => {
            bail!(
                "task {} is already in a terminal state: {:?}. Cannot complete a task that is already done or rejected.",
                needle, task.status
            );
        }
    }

    // Remove the task from the active queue
    let mut completed_task = active.tasks.remove(task_idx);

    // Update the task with completion status and timestamps
    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!("Missing timestamp: current time is required for this operation. Ensure a valid RFC3339 timestamp is provided.");
    }
    OffsetDateTime::parse(now, &Rfc3339).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;

    completed_task.status = status;
    completed_task.updated_at = Some(now.to_string());
    completed_task.completed_at = Some(now.to_string());

    // Append redacted notes
    for note in notes {
        let redacted = redaction::redact_text(note);
        let trimmed = redacted.trim();
        if !trimmed.is_empty() {
            completed_task.notes.push(trimmed.to_string());
        }
    }

    // Load or create the done archive
    let mut done = load_queue_or_default(done_path)?;

    // Validate the combined queue set including the completed task.
    // This avoids false invalid-dependency errors for tasks that depend on
    // the task being completed.
    let mut done_with_completed = done.clone();
    done_with_completed.tasks.push(completed_task.clone());
    validation::validate_queue_set(&active, Some(&done_with_completed), id_prefix, id_width)?;

    // Append the completed task to the done archive
    done.tasks.push(completed_task);

    // Save both files atomically
    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(())
}

pub fn set_status(
    queue: &mut QueueFile,
    task_id: &str,
    status: TaskStatus,
    now_rfc3339: &str,
    note: Option<&str>,
) -> Result<()> {
    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!("Missing timestamp: current time is required for this operation. Ensure a valid RFC3339 timestamp is provided.");
    }
    OffsetDateTime::parse(now, &Rfc3339).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    task.status = status;
    task.updated_at = Some(now.to_string());

    match status {
        TaskStatus::Done | TaskStatus::Rejected => {
            task.completed_at = Some(now.to_string());
        }
        TaskStatus::Todo | TaskStatus::Doing => {
            task.completed_at = None;
        }
    }

    if let Some(note) = note {
        let redacted = redaction::redact_text(note);
        let trimmed = redacted.trim();
        if !trimmed.is_empty() {
            task.notes.push(trimmed.to_string());
        }
    }

    Ok(())
}

pub fn set_field(
    queue: &mut QueueFile,
    task_id: &str,
    key: &str,
    value: &str,
    now_rfc3339: &str,
) -> Result<()> {
    let key_trimmed = key.trim();
    if key_trimmed.is_empty() {
        bail!("Missing custom field key: a key is required for this operation. Provide a valid key (e.g., 'severity').");
    }
    if key_trimmed.chars().any(|c| c.is_whitespace()) {
        bail!(
            "Invalid custom field key: '{}' contains whitespace. Custom field keys must not contain whitespace.",
            key_trimmed
        );
    }

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!("Missing timestamp: current time is required for this operation. Ensure a valid RFC3339 timestamp is provided.");
    }
    OffsetDateTime::parse(now, &Rfc3339).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;

    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    task.custom_fields
        .insert(key_trimmed.to_string(), value.trim().to_string());
    task.updated_at = Some(now.to_string());

    Ok(())
}

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

pub fn next_todo_task(queue: &QueueFile) -> Option<&Task> {
    queue
        .tasks
        .iter()
        .find(|task| task.status == TaskStatus::Todo)
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
        // Since Ord has Critical > High > Medium > Low (semantically),
        // we reverse for descending to put higher priority first
        let ord = if descending {
            a.priority.cmp(&b.priority).reverse()
        } else {
            a.priority.cmp(&b.priority)
        };
        // Use task ID as tiebreaker for stable ordering
        match ord {
            std::cmp::Ordering::Equal => a.id.cmp(&b.id),
            other => other,
        }
    });
}

#[allow(dead_code)]
pub fn delete_task(queue: &mut QueueFile, task_id: &str) -> Result<bool> {
    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let original_len = queue.tasks.len();
    queue.tasks.retain(|t| t.id.trim() != needle);

    let deleted = queue.tasks.len() < original_len;
    if !deleted {
        bail!("task not found: {}", needle);
    }
    Ok(deleted)
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

/// Get all tasks that depend on the given task ID (recursively).
/// Returns a list of task IDs that depend on the root task.
pub fn get_dependents(root_id: &str, active: &QueueFile, done: Option<&QueueFile>) -> Vec<String> {
    let mut dependents = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let root_id = root_id.trim();

    fn collect_dependents(
        task_id: &str,
        active: &QueueFile,
        done: Option<&QueueFile>,
        dependents: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(task_id) {
            return;
        }
        visited.insert(task_id.to_string());

        // Check all tasks in active queue
        for task in &active.tasks {
            let current_id = task.id.trim();
            if task.depends_on.iter().any(|d| d.trim() == task_id) {
                if !dependents.contains(&current_id.to_string()) {
                    dependents.push(current_id.to_string());
                }
                collect_dependents(current_id, active, done, dependents, visited);
            }
        }

        // Check all tasks in done archive
        if let Some(done_file) = done {
            for task in &done_file.tasks {
                let current_id = task.id.trim();
                if task.depends_on.iter().any(|d| d.trim() == task_id) {
                    if !dependents.contains(&current_id.to_string()) {
                        dependents.push(current_id.to_string());
                    }
                    collect_dependents(current_id, active, done, dependents, visited);
                }
            }
        }
    }

    collect_dependents(root_id, active, done, &mut dependents, &mut visited);
    dependents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskPriority, TaskStatus};
    use std::collections::HashMap;

    fn task(id: &str) -> Task {
        task_with(id, TaskStatus::Todo, vec!["code".to_string()])
    }

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            priority: Default::default(),
            tags,
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: vec![],
            custom_fields: HashMap::new(),
        }
    }

    #[test]
    fn set_status_rejects_invalid_rfc3339() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let err =
            set_status(&mut queue, "RQ-0001", TaskStatus::Doing, "invalid", None).unwrap_err();
        assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
        Ok(())
    }

    #[test]
    fn set_status_updates_timestamps_and_fields() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Doing,
            now,
            Some("started"),
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Doing);
        assert_eq!(t.updated_at.as_deref(), Some(now));
        assert_eq!(t.completed_at, None);
        assert_eq!(t.notes, vec!["started".to_string()]);

        let now2 = "2026-01-17T00:02:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Done,
            now2,
            Some("completed"),
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Done);
        assert_eq!(t.updated_at.as_deref(), Some(now2));
        assert_eq!(t.completed_at.as_deref(), Some(now2));
        assert!(t.notes.iter().any(|n| n == "completed"));

        Ok(())
    }

    #[test]
    fn set_status_redacts_note() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Doing,
            now,
            Some("API_KEY=abc12345"),
        )?;

        let t = &queue.tasks[0];
        assert_eq!(t.notes, vec!["API_KEY=[REDACTED]".to_string()]);

        Ok(())
    }

    #[test]
    fn set_status_sanitizes_leading_backticks() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Doing,
            now,
            Some("`make ci` failed"),
        )?;

        let t = &queue.tasks[0];
        assert_eq!(t.notes, vec!["`make ci` failed".to_string()]);

        Ok(())
    }

    #[test]
    fn backfill_missing_fields_populates_request() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].request = None;

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T00:00:00Z",
        );

        assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_populates_timestamps() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].created_at = None;
        queue.tasks[0].updated_at = None;

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_skips_existing_values() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "new request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(queue.tasks[0].request, Some("test request".to_string()));
        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T00:00:00Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T00:00:00Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_only_affects_specified_ids() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.request = None;
        let t2 = task("RQ-0002");
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![t1, t2],
        };

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "backfilled request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(
            queue.tasks[0].request,
            Some("backfilled request".to_string())
        );
        assert_eq!(queue.tasks[1].request, Some("test request".to_string()));
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_handles_empty_string_as_missing() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].request = Some("".to_string());
        queue.tasks[0].created_at = Some("".to_string());
        queue.tasks[0].updated_at = Some("".to_string());

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_empty_now_skips() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].created_at = None;
        queue.tasks[0].updated_at = None;

        backfill_missing_fields(&mut queue, &["RQ-0001".to_string()], "default request", "");

        assert_eq!(queue.tasks[0].created_at, None);
        assert_eq!(queue.tasks[0].updated_at, None);
        Ok(())
    }

    #[test]
    fn sort_tasks_by_priority_descending() {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec![]),
                task_with("RQ-0002", TaskStatus::Todo, vec![]),
                task_with("RQ-0003", TaskStatus::Todo, vec![]),
            ],
        };
        queue.tasks[0].priority = TaskPriority::Low;
        queue.tasks[1].priority = TaskPriority::Critical;
        queue.tasks[2].priority = TaskPriority::High;

        sort_tasks_by_priority(&mut queue, true);

        assert_eq!(queue.tasks[0].id, "RQ-0002"); // Critical first
        assert_eq!(queue.tasks[1].id, "RQ-0003"); // High second
        assert_eq!(queue.tasks[2].id, "RQ-0001"); // Low last
    }

    #[test]
    fn sort_tasks_by_priority_ascending() {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec![]),
                task_with("RQ-0002", TaskStatus::Todo, vec![]),
                task_with("RQ-0003", TaskStatus::Todo, vec![]),
            ],
        };
        queue.tasks[0].priority = TaskPriority::Low;
        queue.tasks[1].priority = TaskPriority::Critical;
        queue.tasks[2].priority = TaskPriority::High;

        sort_tasks_by_priority(&mut queue, false);

        assert_eq!(queue.tasks[0].id, "RQ-0001"); // Low first
        assert_eq!(queue.tasks[1].id, "RQ-0003"); // High second
        assert_eq!(queue.tasks[2].id, "RQ-0002"); // Critical last
    }

    #[test]
    fn complete_task_moves_task_from_queue_to_done() -> Result<()> {
        use tempfile::TempDir;

        // Create a temp directory to hold queue and done files
        let temp_dir = TempDir::new()?;
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "doing",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
        std::fs::write(&queue_path, queue_json)?;

        let now = "2026-01-20T12:00:00Z";
        complete_task(
            &queue_path,
            &done_path,
            "RQ-0001",
            TaskStatus::Done,
            now,
            &["Test note".to_string()],
            "RQ",
            4,
        )?;

        // Verify task was removed from queue
        let queue_content = std::fs::read_to_string(&queue_path)?;
        let queue: QueueFile = serde_json::from_str(&queue_content)?;
        assert_eq!(queue.tasks.len(), 0);

        // Verify task was added to done with correct status
        let done_content = std::fs::read_to_string(&done_path)?;
        let done: QueueFile = serde_json::from_str(&done_content)?;
        assert_eq!(done.tasks.len(), 1);
        assert_eq!(done.tasks[0].id, "RQ-0001");
        assert_eq!(done.tasks[0].status, TaskStatus::Done);
        assert_eq!(done.tasks[0].completed_at.as_deref(), Some(now));
        assert_eq!(done.tasks[0].updated_at.as_deref(), Some(now));
        assert_eq!(done.tasks[0].notes, vec!["Test note"]);

        Ok(())
    }

    #[test]
    fn complete_task_allows_dependents_in_active_queue() -> Result<()> {
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "doing",
                    "title": "Dependency task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                },
                {
                    "id": "RQ-0002",
                    "status": "todo",
                    "title": "Dependent task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": ["RQ-0001"],
                    "custom_fields": {}
                }
            ]
        }"#;
        std::fs::write(&queue_path, queue_json)?;

        let now = "2026-01-20T12:00:00Z";
        complete_task(
            &queue_path,
            &done_path,
            "RQ-0001",
            TaskStatus::Done,
            now,
            &[],
            "RQ",
            4,
        )?;

        let queue_content = std::fs::read_to_string(&queue_path)?;
        let queue: QueueFile = serde_json::from_str(&queue_content)?;
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0002");

        let done_content = std::fs::read_to_string(&done_path)?;
        let done: QueueFile = serde_json::from_str(&done_content)?;
        assert_eq!(done.tasks.len(), 1);
        assert_eq!(done.tasks[0].id, "RQ-0001");

        Ok(())
    }

    #[test]
    fn complete_task_rejects_non_terminal_status() -> Result<()> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut queue_file = NamedTempFile::new()?;
        let done_file = NamedTempFile::new()?;

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "doing",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
        queue_file.write_all(queue_json.as_bytes())?;
        queue_file.flush()?;

        let now = "2026-01-20T12:00:00Z";
        let err = complete_task(
            queue_file.path(),
            done_file.path(),
            "RQ-0001",
            TaskStatus::Todo, // Invalid - not a terminal status
            now,
            &[],
            "RQ",
            4,
        )
        .unwrap_err();
        assert!(format!("{err}")
            .to_lowercase()
            .contains("invalid completion status"));

        Ok(())
    }

    #[test]
    fn complete_task_rejects_task_already_terminal() -> Result<()> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut queue_file = NamedTempFile::new()?;
        let done_file = NamedTempFile::new()?;

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "done",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "completed_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
        queue_file.write_all(queue_json.as_bytes())?;
        queue_file.flush()?;

        let now = "2026-01-20T12:00:00Z";
        let err = complete_task(
            queue_file.path(),
            done_file.path(),
            "RQ-0001",
            TaskStatus::Done,
            now,
            &[],
            "RQ",
            4,
        )
        .unwrap_err();
        assert!(format!("{err}")
            .to_lowercase()
            .contains("already in a terminal state"));

        Ok(())
    }

    #[test]
    fn complete_task_rejects_nonexistent_task() -> Result<()> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut queue_file = NamedTempFile::new()?;
        let done_file = NamedTempFile::new()?;

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0002",
                    "status": "todo",
                    "title": "Other task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
        queue_file.write_all(queue_json.as_bytes())?;
        queue_file.flush()?;

        let now = "2026-01-20T12:00:00Z";
        let err = complete_task(
            queue_file.path(),
            done_file.path(),
            "RQ-0001", // Does not exist
            TaskStatus::Done,
            now,
            &[],
            "RQ",
            4,
        )
        .unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("task not found"));

        Ok(())
    }
}
