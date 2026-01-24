//! Task edit helpers shared by CLI and TUI.

use super::validate::parse_rfc3339_utc;
use crate::contracts::{QueueFile, TaskPriority, TaskStatus};
use crate::queue;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

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
    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let index = queue
        .tasks
        .iter()
        .position(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    let previous = queue
        .tasks
        .get(index)
        .cloned()
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    let task = queue
        .tasks
        .get_mut(index)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    let trimmed = input.trim();

    match key {
        TaskEditKey::Title => {
            if trimmed.is_empty() {
                bail!("Title cannot be empty");
            }
            task.title = trimmed.to_string();
        }
        TaskEditKey::Status => {
            let next_status = if trimmed.is_empty() {
                cycle_status(task.status)
            } else {
                parse_status(trimmed)?
            };
            let now = ensure_now(now_rfc3339)?;
            queue::apply_status_policy(task, next_status, &now, None)?;
        }
        TaskEditKey::Priority => {
            task.priority = if trimmed.is_empty() {
                task.priority.cycle()
            } else {
                parse_priority(trimmed)?
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
            task.custom_fields = parse_custom_fields(trimmed)?;
        }
        TaskEditKey::CreatedAt => {
            validate_rfc3339_input("created_at", trimmed)?;
            task.created_at = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        TaskEditKey::UpdatedAt => {
            validate_rfc3339_input("updated_at", trimmed)?;
            task.updated_at = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        TaskEditKey::CompletedAt => {
            validate_rfc3339_input("completed_at", trimmed)?;
            task.completed_at = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
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

fn parse_custom_fields(input: &str) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    if input.trim().is_empty() {
        return Ok(map);
    }

    for raw in input.split([',', '\n']) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (key, value) = trimmed
            .split_once('=')
            .ok_or_else(|| anyhow!("Custom field entry must be key=value"))?;
        let key = key.trim();
        if key.is_empty() {
            bail!("Custom field key cannot be empty");
        }
        if key.chars().any(|c| c.is_whitespace()) {
            bail!("Custom field keys cannot contain whitespace");
        }
        let value = value.trim();
        map.insert(key.to_string(), value.to_string());
    }
    Ok(map)
}

fn validate_rfc3339_input(label: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    OffsetDateTime::parse(trimmed, &Rfc3339)
        .with_context(|| format!("{} must be a valid RFC3339 timestamp", label))?;
    Ok(())
}
