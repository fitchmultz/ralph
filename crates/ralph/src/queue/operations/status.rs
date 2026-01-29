//! Status mutation helpers for queue tasks.

use super::validate::parse_rfc3339_utc;
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue::{load_queue, load_queue_or_default, save_queue, validation};
use crate::redaction;
use anyhow::{anyhow, bail, Result};
use std::path::Path;

/// Apply the shared status-transition policy to a task.
///
/// This updates status, updated_at, and completed_at based on terminal states,
/// and optionally appends a redacted note.
pub fn apply_status_policy(
    task: &mut Task,
    status: TaskStatus,
    now_rfc3339: &str,
    note: Option<&str>,
) -> Result<()> {
    apply_status_fields(task, status, now_rfc3339)?;

    if let Some(note) = note {
        append_redacted_note(task, note);
    }

    Ok(())
}

fn apply_status_fields(task: &mut Task, status: TaskStatus, now_rfc3339: &str) -> Result<()> {
    let now = parse_rfc3339_utc(now_rfc3339)?;

    task.status = status;
    task.updated_at = Some(now.clone());

    match status {
        TaskStatus::Done | TaskStatus::Rejected => {
            // Preserve an existing completed_at (e.g., manual backfill) but ensure
            // terminal tasks never lack a completion timestamp.
            if task
                .completed_at
                .as_ref()
                .is_none_or(|t| t.trim().is_empty())
            {
                task.completed_at = Some(now.clone());
            }
        }
        TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing => {
            // Non-terminal tasks must not carry a completed timestamp.
            task.completed_at = None;
        }
    }

    Ok(())
}

fn append_redacted_note(task: &mut Task, note: &str) {
    let redacted = redaction::redact_text(note);
    let trimmed = redacted.trim();
    if !trimmed.is_empty() {
        task.notes.push(trimmed.to_string());
    }
}

fn append_redacted_notes(task: &mut Task, notes: &[String]) {
    for note in notes {
        append_redacted_note(task, note);
    }
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
    max_dependency_depth: u8,
) -> Result<()> {
    match status {
        TaskStatus::Done | TaskStatus::Rejected => {}
        TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing => {
            bail!(
            "Invalid completion status: only 'done' or 'rejected' are allowed. Got: {:?}. Use 'ralph task done {}' or 'ralph task reject {}'.",
            status, task_id, task_id
        );
        }
    }

    let mut active = load_queue(queue_path)?;
    validation::validate_queue(&active, id_prefix, id_width)?;

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

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

    match task.status {
        TaskStatus::Todo | TaskStatus::Doing => {}
        TaskStatus::Draft => {
            bail!(
                "task {} is still in draft status. Promote it to todo before completing.",
                needle
            );
        }
        TaskStatus::Done | TaskStatus::Rejected => {
            bail!(
                "task {} is already in a terminal state: {:?}. Cannot complete a task that is already done or rejected.",
                needle, task.status
            );
        }
    }

    let mut completed_task = active.tasks.remove(task_idx);

    apply_status_fields(&mut completed_task, status, now_rfc3339)?;
    append_redacted_notes(&mut completed_task, notes);

    let mut done = load_queue_or_default(done_path)?;

    let mut done_with_completed = done.clone();
    done_with_completed.tasks.push(completed_task.clone());
    let warnings = validation::validate_queue_set(
        &active,
        Some(&done_with_completed),
        id_prefix,
        id_width,
        max_dependency_depth,
    )?;
    validation::log_warnings(&warnings);

    done.tasks.push(completed_task);

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
    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    apply_status_policy(task, status, now_rfc3339, note)?;

    Ok(())
}

pub fn promote_draft_to_todo(
    queue: &mut QueueFile,
    task_id: &str,
    now_rfc3339: &str,
    note: Option<&str>,
) -> Result<()> {
    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let task = queue
        .tasks
        .iter()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    if task.status != TaskStatus::Draft {
        bail!(
            "task {} is not in draft status (current status: {}). Only draft tasks can be marked ready.",
            needle,
            task.status
        );
    }

    set_status(queue, needle, TaskStatus::Todo, now_rfc3339, note)
}
