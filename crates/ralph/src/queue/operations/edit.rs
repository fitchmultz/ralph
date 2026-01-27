//! Task edit helpers shared by CLI and TUI.
//!
//! Responsibilities:
//! - Apply edits to a single task and update related timestamps.
//! - Parse and validate edit input (status, priority, custom fields, RFC3339 values).
//!
//! Does not handle:
//! - Persisting queue files or locating tasks outside the provided queue.
//! - Cross-task dependency resolution beyond status policy checks.
//!
//! Assumptions/invariants:
//! - Callers provide a loaded `QueueFile` and a valid RFC3339 `now` value.
//! - Task IDs are matched after trimming and are case-sensitive.

use super::validate::{ensure_task_id, parse_custom_fields_with_context, parse_rfc3339_utc};
use crate::contracts::{QueueFile, TaskPriority, TaskStatus};
use crate::queue;
use crate::timeutil;
use anyhow::{anyhow, bail, Context, Result};
use time::UtcOffset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskEditKey {
    Title,
    Status,
    Priority,
    Tags,
    Scope,
    Evidence,
    Plan,
    Notes,
    Request,
    DependsOn,
    CustomFields,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
}

impl TaskEditKey {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskEditKey::Title => "title",
            TaskEditKey::Status => "status",
            TaskEditKey::Priority => "priority",
            TaskEditKey::Tags => "tags",
            TaskEditKey::Scope => "scope",
            TaskEditKey::Evidence => "evidence",
            TaskEditKey::Plan => "plan",
            TaskEditKey::Notes => "notes",
            TaskEditKey::Request => "request",
            TaskEditKey::DependsOn => "depends_on",
            TaskEditKey::CustomFields => "custom_fields",
            TaskEditKey::CreatedAt => "created_at",
            TaskEditKey::UpdatedAt => "updated_at",
            TaskEditKey::CompletedAt => "completed_at",
        }
    }
}

impl std::str::FromStr for TaskEditKey {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let normalized = value.trim().to_lowercase();
        match normalized.as_str() {
            "title" => Ok(TaskEditKey::Title),
            "status" => Ok(TaskEditKey::Status),
            "priority" => Ok(TaskEditKey::Priority),
            "tags" => Ok(TaskEditKey::Tags),
            "scope" => Ok(TaskEditKey::Scope),
            "evidence" => Ok(TaskEditKey::Evidence),
            "plan" => Ok(TaskEditKey::Plan),
            "notes" => Ok(TaskEditKey::Notes),
            "request" => Ok(TaskEditKey::Request),
            "depends_on" => Ok(TaskEditKey::DependsOn),
            "custom_fields" => Ok(TaskEditKey::CustomFields),
            "created_at" => Ok(TaskEditKey::CreatedAt),
            "updated_at" => Ok(TaskEditKey::UpdatedAt),
            "completed_at" => Ok(TaskEditKey::CompletedAt),
            _ => bail!(
                "Unknown task field: '{}'. Expected one of: title, status, priority, tags, scope, evidence, plan, notes, request, depends_on, custom_fields, created_at, updated_at, completed_at.",
                value
            ),
        }
    }
}

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
) -> Result<()> {
    let operation = "edit";
    let needle = ensure_task_id(task_id, operation)?;

    let index = queue
        .tasks
        .iter()
        .position(|t| t.id.trim() == needle)
        .ok_or_else(|| {
            anyhow!(
                "Queue edit failed (task_id={}): task not found in .ralph/queue.json.",
                needle
            )
        })?;

    let previous = queue.tasks.get(index).cloned().ok_or_else(|| {
        anyhow!(
            "Queue edit failed (task_id={}): task not found in .ralph/queue.json.",
            needle
        )
    })?;

    let task = queue.tasks.get_mut(index).ok_or_else(|| {
        anyhow!(
            "Queue edit failed (task_id={}): task not found in .ralph/queue.json.",
            needle
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
                parse_priority(trimmed).with_context(|| {
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
        TaskEditKey::CustomFields => {
            task.custom_fields = parse_custom_fields_with_context(needle, trimmed, operation)?;
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
    }

    if !matches!(key, TaskEditKey::UpdatedAt) {
        let now = ensure_now(now_rfc3339)?;
        task.updated_at = Some(now.to_string());
    }

    if let Err(err) = queue::validate_queue_set(queue, done, id_prefix, id_width) {
        queue.tasks[index] = previous;
        return Err(err);
    }

    Ok(())
}

fn ensure_now(now_rfc3339: &str) -> Result<String> {
    parse_rfc3339_utc(now_rfc3339)
}

fn cycle_status(status: TaskStatus) -> TaskStatus {
    match status {
        TaskStatus::Draft => TaskStatus::Todo,
        TaskStatus::Todo => TaskStatus::Doing,
        TaskStatus::Doing => TaskStatus::Done,
        TaskStatus::Done => TaskStatus::Rejected,
        TaskStatus::Rejected => TaskStatus::Draft,
    }
}

fn parse_status(value: &str) -> Result<TaskStatus> {
    match value.trim().to_lowercase().as_str() {
        "draft" => Ok(TaskStatus::Draft),
        "todo" => Ok(TaskStatus::Todo),
        "doing" => Ok(TaskStatus::Doing),
        "done" => Ok(TaskStatus::Done),
        "rejected" => Ok(TaskStatus::Rejected),
        _ => bail!(
            "Invalid status: '{}'. Expected one of: draft, todo, doing, done, rejected.",
            value
        ),
    }
}

fn parse_priority(value: &str) -> Result<TaskPriority> {
    match value.trim().to_lowercase().as_str() {
        "critical" => Ok(TaskPriority::Critical),
        "high" => Ok(TaskPriority::High),
        "medium" => Ok(TaskPriority::Medium),
        "low" => Ok(TaskPriority::Low),
        _ => bail!(
            "Invalid priority: '{}'. Expected one of: critical, high, medium, low.",
            value
        ),
    }
}

fn parse_list(input: &str) -> Vec<String> {
    input
        .split([',', '\n'])
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn normalize_rfc3339_input(label: &str, value: &str) -> Result<Option<String>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let dt = timeutil::parse_rfc3339(trimmed)
        .with_context(|| format!("{} must be a valid RFC3339 timestamp", label))?;
    if dt.offset() != UtcOffset::UTC {
        bail!("{} must be a valid RFC3339 UTC timestamp", label);
    }
    let formatted = timeutil::format_rfc3339(dt)
        .with_context(|| format!("{} must be a valid RFC3339 timestamp", label))?;
    Ok(Some(formatted))
}
