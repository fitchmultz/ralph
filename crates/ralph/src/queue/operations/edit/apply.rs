//! Task edit application logic.
//!
//! Responsibilities:
//! - Apply edits to task fields with proper validation.
//! - Update timestamps and maintain task consistency.
//! - Rollback changes if validation fails.
//!
//! Does not handle:
//! - Queue persistence (callers must save the queue after editing).
//! - Previewing changes without applying them (see `preview.rs`).
//!
//! Assumptions/invariants:
//! - Callers provide a valid RFC3339 `now` value for timestamp updates.
//! - Task IDs are matched after trimming and are case-sensitive.
//! - Failed validation rolls back the task to its previous state.

use super::key::TaskEditKey;
use super::parsing::{
    cycle_status, normalize_rfc3339_input, parse_list, parse_status, parse_task_agent_override,
};
use super::validate_input::ensure_now;
use crate::contracts::{QueueFile, TaskPriority};
use crate::queue;
use crate::queue::operations::validate::{ensure_task_id, parse_custom_fields_with_context};
use anyhow::{Context, Result, anyhow, bail};

#[allow(clippy::too_many_arguments)]
pub fn apply_task_edit(
    queue: &mut QueueFile,
    done: Option<&QueueFile>,
    task_id: &str,
    key: TaskEditKey,
    input: &str,
    now_rfc3339: &str,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<()> {
    let operation = "edit";
    let needle = ensure_task_id(task_id, operation)?;

    let index = queue
        .tasks
        .iter()
        .position(|t| t.id.trim() == needle)
        .ok_or_else(|| {
            anyhow!(
                "{}",
                crate::error_messages::task_not_found_for_edit(operation, needle)
            )
        })?;

    let previous = queue.tasks.get(index).cloned().ok_or_else(|| {
        anyhow!(
            "{}",
            crate::error_messages::task_not_found_for_edit(operation, needle)
        )
    })?;

    let task = queue.tasks.get_mut(index).ok_or_else(|| {
        anyhow!(
            "{}",
            crate::error_messages::task_not_found_for_edit(operation, needle)
        )
    })?;

    let trimmed = input.trim();

    match key {
        TaskEditKey::Title => {
            if trimmed.is_empty() {
                bail!(
                    "Queue edit failed (task_id={}, field=title): title cannot be empty. Provide a non-empty title (e.g., 'Fix login bug').",
                    needle
                );
            }
            task.title = trimmed.to_string();
        }
        TaskEditKey::Status => {
            let next_status = if trimmed.is_empty() {
                cycle_status(task.status)
            } else {
                parse_status(trimmed).with_context(|| {
                    format!("Queue edit failed (task_id={}, field=status)", needle)
                })?
            };
            let now = ensure_now(now_rfc3339)?;
            queue::apply_status_policy(task, next_status, &now, None)?;
        }
        TaskEditKey::Priority => {
            task.priority = if trimmed.is_empty() {
                task.priority.cycle()
            } else {
                trimmed.parse::<TaskPriority>().with_context(|| {
                    format!("Queue edit failed (task_id={}, field=priority)", needle)
                })?
            };
        }
        TaskEditKey::Tags => {
            task.tags = parse_list(trimmed);
        }
        TaskEditKey::Scope => {
            task.scope = parse_list(trimmed);
        }
        TaskEditKey::Evidence => {
            task.evidence = parse_list(trimmed);
        }
        TaskEditKey::Plan => {
            task.plan = parse_list(trimmed);
        }
        TaskEditKey::Notes => {
            task.notes = parse_list(trimmed);
        }
        TaskEditKey::Request => {
            task.request = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        TaskEditKey::DependsOn => {
            task.depends_on = parse_list(trimmed);
        }
        TaskEditKey::Blocks => {
            task.blocks = parse_list(trimmed);
        }
        TaskEditKey::RelatesTo => {
            task.relates_to = parse_list(trimmed);
        }
        TaskEditKey::Duplicates => {
            task.duplicates = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        TaskEditKey::CustomFields => {
            task.custom_fields = parse_custom_fields_with_context(needle, trimmed, operation)?;
        }
        TaskEditKey::Agent => {
            task.agent = parse_task_agent_override(trimmed)
                .with_context(|| format!("Queue edit failed (task_id={}, field=agent)", needle))?;
        }
        TaskEditKey::CreatedAt => {
            let normalized = normalize_rfc3339_input("created_at", trimmed).with_context(|| {
                format!("Queue edit failed (task_id={}, field=created_at)", needle)
            })?;
            task.created_at = normalized;
        }
        TaskEditKey::UpdatedAt => {
            let normalized = normalize_rfc3339_input("updated_at", trimmed).with_context(|| {
                format!("Queue edit failed (task_id={}, field=updated_at)", needle)
            })?;
            task.updated_at = normalized;
        }
        TaskEditKey::CompletedAt => {
            let normalized =
                normalize_rfc3339_input("completed_at", trimmed).with_context(|| {
                    format!("Queue edit failed (task_id={}, field=completed_at)", needle)
                })?;
            task.completed_at = normalized;
        }
        TaskEditKey::StartedAt => {
            let normalized = normalize_rfc3339_input("started_at", trimmed).with_context(|| {
                format!("Queue edit failed (task_id={}, field=started_at)", needle)
            })?;
            task.started_at = normalized;
        }
        TaskEditKey::ScheduledStart => {
            let normalized =
                normalize_rfc3339_input("scheduled_start", trimmed).with_context(|| {
                    format!(
                        "Queue edit failed (task_id={}, field=scheduled_start)",
                        needle
                    )
                })?;
            task.scheduled_start = normalized;
        }
        TaskEditKey::EstimatedMinutes => {
            let minutes = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.parse::<u32>().with_context(|| {
                    format!(
                        "Queue edit failed (task_id={}, field=estimated_minutes): must be a non-negative integer",
                        needle
                    )
                })?)
            };
            task.estimated_minutes = minutes;
        }
        TaskEditKey::ActualMinutes => {
            let minutes = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.parse::<u32>().with_context(|| {
                    format!(
                        "Queue edit failed (task_id={}, field=actual_minutes): must be a non-negative integer",
                        needle
                    )
                })?)
            };
            task.actual_minutes = minutes;
        }
    }

    if !matches!(key, TaskEditKey::UpdatedAt) {
        let now = ensure_now(now_rfc3339)?;
        task.updated_at = Some(now.to_string());
    }

    match queue::validate_queue_set(queue, done, id_prefix, id_width, max_dependency_depth) {
        Ok(warnings) => {
            queue::log_warnings(&warnings);
        }
        Err(err) => {
            queue.tasks[index] = previous;
            return Err(err);
        }
    }

    Ok(())
}
