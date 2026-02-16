//! Task edit preview functionality.
//!
//! Responsibilities:
//! - Preview task edit changes without applying them to the actual queue.
//! - Generate a TaskEditPreview showing old/new values and any validation warnings.
//!
//! Does not handle:
//! - Actually modifying the queue (see `apply.rs`).
//!
//! Assumptions/invariants:
//! - Creates a clone of the queue to simulate edits without side effects.
//! - Validation warnings are collected and returned in the preview.

use super::key::TaskEditKey;
use super::parsing::{
    cycle_status, normalize_rfc3339_input, parse_list, parse_status, parse_task_agent_override,
};
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue;
use crate::queue::ValidationWarning;
use crate::queue::operations::validate::{ensure_task_id, parse_custom_fields_with_context};
use anyhow::{Context, Result, anyhow, bail};

/// Preview of what would change in a task edit operation.
///
/// Used by dry-run mode to show changes without applying them.
#[derive(Debug, Clone)]
pub struct TaskEditPreview {
    pub task_id: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub warnings: Vec<ValidationWarning>,
}

/// Preview task edit changes without applying them.
///
/// Clones the queue, applies the edit to the clone, validates the result,
/// and returns a preview describing what would change.
///
/// # Arguments
/// * `queue` - The queue file containing the task to edit
/// * `done` - Optional done file for validation
/// * `task_id` - ID of the task to edit
/// * `key` - Field to edit
/// * `input` - New value for the field
/// * `now_rfc3339` - Current timestamp for updated_at
/// * `id_prefix` - Task ID prefix for validation
/// * `id_width` - Task ID width for validation
/// * `max_dependency_depth` - Maximum dependency depth for validation
///
/// # Returns
/// A `TaskEditPreview` describing the changes that would be made.
#[allow(clippy::too_many_arguments)]
pub fn preview_task_edit(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    task_id: &str,
    key: TaskEditKey,
    input: &str,
    now_rfc3339: &str,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<TaskEditPreview> {
    let operation = "preview edit";
    let needle = ensure_task_id(task_id, operation)?;

    // Find the task
    let task = queue
        .tasks
        .iter()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| {
            anyhow!(
                "Queue edit preview failed (task_id={}): task not found in .ralph/queue.json.",
                needle
            )
        })?;

    // Clone the task to simulate the edit
    let mut preview_task = task.clone();
    let trimmed = input.trim();

    // Apply the edit to the cloned task (similar to apply_task_edit but on clone)
    match key {
        TaskEditKey::Title => {
            if trimmed.is_empty() {
                bail!(
                    "Queue edit preview failed (task_id={}, field=title): title cannot be empty. Provide a non-empty title (e.g., 'Fix login bug').",
                    needle
                );
            }
            preview_task.title = trimmed.to_string();
        }
        TaskEditKey::Status => {
            let next_status = if trimmed.is_empty() {
                cycle_status(preview_task.status)
            } else {
                parse_status(trimmed).with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=status)",
                        needle
                    )
                })?
            };
            preview_task.status = next_status;
            if next_status == TaskStatus::Done || next_status == TaskStatus::Rejected {
                preview_task.completed_at = Some(now_rfc3339.to_string());
            } else {
                preview_task.completed_at = None;
            }
        }
        TaskEditKey::Priority => {
            preview_task.priority = if trimmed.is_empty() {
                preview_task.priority.cycle()
            } else {
                trimmed.parse::<TaskPriority>().with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=priority)",
                        needle
                    )
                })?
            };
        }
        TaskEditKey::Tags => {
            preview_task.tags = parse_list(trimmed);
        }
        TaskEditKey::Scope => {
            preview_task.scope = parse_list(trimmed);
        }
        TaskEditKey::Evidence => {
            preview_task.evidence = parse_list(trimmed);
        }
        TaskEditKey::Plan => {
            preview_task.plan = parse_list(trimmed);
        }
        TaskEditKey::Notes => {
            preview_task.notes = parse_list(trimmed);
        }
        TaskEditKey::Request => {
            preview_task.request = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        TaskEditKey::DependsOn => {
            preview_task.depends_on = parse_list(trimmed);
        }
        TaskEditKey::Blocks => {
            preview_task.blocks = parse_list(trimmed);
        }
        TaskEditKey::RelatesTo => {
            preview_task.relates_to = parse_list(trimmed);
        }
        TaskEditKey::Duplicates => {
            preview_task.duplicates = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        TaskEditKey::CustomFields => {
            preview_task.custom_fields =
                parse_custom_fields_with_context(needle, trimmed, operation)?;
        }
        TaskEditKey::Agent => {
            preview_task.agent = parse_task_agent_override(trimmed).with_context(|| {
                format!(
                    "Queue edit preview failed (task_id={}, field=agent)",
                    needle
                )
            })?;
        }
        TaskEditKey::CreatedAt => {
            let normalized = normalize_rfc3339_input("created_at", trimmed).with_context(|| {
                format!(
                    "Queue edit preview failed (task_id={}, field=created_at)",
                    needle
                )
            })?;
            preview_task.created_at = normalized;
        }
        TaskEditKey::UpdatedAt => {
            let normalized = normalize_rfc3339_input("updated_at", trimmed).with_context(|| {
                format!(
                    "Queue edit preview failed (task_id={}, field=updated_at)",
                    needle
                )
            })?;
            preview_task.updated_at = normalized;
        }
        TaskEditKey::CompletedAt => {
            let normalized =
                normalize_rfc3339_input("completed_at", trimmed).with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=completed_at)",
                        needle
                    )
                })?;
            preview_task.completed_at = normalized;
        }
        TaskEditKey::StartedAt => {
            let normalized = normalize_rfc3339_input("started_at", trimmed).with_context(|| {
                format!(
                    "Queue edit preview failed (task_id={}, field=started_at)",
                    needle
                )
            })?;
            preview_task.started_at = normalized;
        }
        TaskEditKey::ScheduledStart => {
            let normalized =
                normalize_rfc3339_input("scheduled_start", trimmed).with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=scheduled_start)",
                        needle
                    )
                })?;
            preview_task.scheduled_start = normalized;
        }
        TaskEditKey::EstimatedMinutes => {
            let minutes = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.parse::<u32>().with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=estimated_minutes): must be a non-negative integer",
                        needle
                    )
                })?)
            };
            preview_task.estimated_minutes = minutes;
        }
        TaskEditKey::ActualMinutes => {
            let minutes = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.parse::<u32>().with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=actual_minutes): must be a non-negative integer",
                        needle
                    )
                })?)
            };
            preview_task.actual_minutes = minutes;
        }
    }

    // Update timestamp unless we're setting updated_at explicitly
    if !matches!(key, TaskEditKey::UpdatedAt) {
        preview_task.updated_at = Some(now_rfc3339.to_string());
    }

    // Validate the modified task by creating a temporary queue
    let mut preview_queue = queue.clone();
    if let Some(index) = preview_queue
        .tasks
        .iter()
        .position(|t| t.id.trim() == needle)
    {
        preview_queue.tasks[index] = preview_task.clone();
    }

    let warnings = match queue::validate_queue_set(
        &preview_queue,
        done,
        id_prefix,
        id_width,
        max_dependency_depth,
    ) {
        Ok(warnings) => warnings,
        Err(err) => {
            bail!(
                "Queue edit preview failed (task_id={}): validation error - {}",
                needle,
                err
            );
        }
    };

    // Format old and new values for display
    let old_value = format_field_value(task, key);
    let new_value = format_field_value(&preview_task, key);

    Ok(TaskEditPreview {
        task_id: needle.to_string(),
        field: key.as_str().to_string(),
        old_value,
        new_value,
        warnings,
    })
}

/// Format a field value for display in previews.
///
/// Uses semicolon separator for Evidence, Plan, Notes (longer text items)
/// and comma separator for other list fields.
pub(crate) fn format_field_value(task: &Task, key: TaskEditKey) -> String {
    let sep = match key {
        TaskEditKey::Evidence | TaskEditKey::Plan | TaskEditKey::Notes => "; ",
        _ => ", ",
    };
    key.format_value(task, sep)
}
