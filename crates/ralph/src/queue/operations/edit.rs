//! Task edit helpers shared by CLI and GUI clients.
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
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue;
use crate::queue::ValidationWarning;
use crate::timeutil;
use anyhow::{Context, Result, anyhow, bail};
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
    Blocks,
    RelatesTo,
    Duplicates,
    CustomFields,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
    StartedAt,
    ScheduledStart,
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
            TaskEditKey::Blocks => "blocks",
            TaskEditKey::RelatesTo => "relates_to",
            TaskEditKey::Duplicates => "duplicates",
            TaskEditKey::CustomFields => "custom_fields",
            TaskEditKey::CreatedAt => "created_at",
            TaskEditKey::UpdatedAt => "updated_at",
            TaskEditKey::CompletedAt => "completed_at",
            TaskEditKey::StartedAt => "started_at",
            TaskEditKey::ScheduledStart => "scheduled_start",
        }
    }

    /// Returns whether this field is a list type (Vec<String>).
    pub fn is_list_field(self) -> bool {
        matches!(
            self,
            TaskEditKey::Tags
                | TaskEditKey::Scope
                | TaskEditKey::Evidence
                | TaskEditKey::Plan
                | TaskEditKey::Notes
                | TaskEditKey::DependsOn
                | TaskEditKey::Blocks
                | TaskEditKey::RelatesTo
        )
    }

    /// Format this field's value from a task with the given list separator.
    ///
    /// For list fields, elements are joined with the provided separator.
    /// For optional fields, returns empty string when None.
    pub fn format_value(self, task: &Task, list_sep: &str) -> String {
        match self {
            TaskEditKey::Title => task.title.clone(),
            TaskEditKey::Status => task.status.to_string(),
            TaskEditKey::Priority => task.priority.to_string(),
            TaskEditKey::Tags => task.tags.join(list_sep),
            TaskEditKey::Scope => task.scope.join(list_sep),
            TaskEditKey::Evidence => task.evidence.join(list_sep),
            TaskEditKey::Plan => task.plan.join(list_sep),
            TaskEditKey::Notes => task.notes.join(list_sep),
            TaskEditKey::Request => task.request.clone().unwrap_or_default(),
            TaskEditKey::DependsOn => task.depends_on.join(list_sep),
            TaskEditKey::Blocks => task.blocks.join(list_sep),
            TaskEditKey::RelatesTo => task.relates_to.join(list_sep),
            TaskEditKey::Duplicates => task.duplicates.clone().unwrap_or_default(),
            TaskEditKey::CustomFields => {
                let pairs: Vec<String> = task
                    .custom_fields
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                pairs.join(list_sep)
            }
            TaskEditKey::CreatedAt => task.created_at.clone().unwrap_or_default(),
            TaskEditKey::UpdatedAt => task.updated_at.clone().unwrap_or_default(),
            TaskEditKey::CompletedAt => task.completed_at.clone().unwrap_or_default(),
            TaskEditKey::StartedAt => task.started_at.clone().unwrap_or_default(),
            TaskEditKey::ScheduledStart => task.scheduled_start.clone().unwrap_or_default(),
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
            "blocks" => Ok(TaskEditKey::Blocks),
            "relates_to" => Ok(TaskEditKey::RelatesTo),
            "duplicates" => Ok(TaskEditKey::Duplicates),
            "custom_fields" => Ok(TaskEditKey::CustomFields),
            "created_at" => Ok(TaskEditKey::CreatedAt),
            "updated_at" => Ok(TaskEditKey::UpdatedAt),
            "completed_at" => Ok(TaskEditKey::CompletedAt),
            "started_at" => Ok(TaskEditKey::StartedAt),
            "scheduled_start" => Ok(TaskEditKey::ScheduledStart),
            _ => bail!(
                "Unknown task field: '{}'. Expected one of: title, status, priority, tags, scope, evidence, plan, notes, request, depends_on, blocks, relates_to, duplicates, custom_fields, created_at, updated_at, completed_at, started_at, scheduled_start.",
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
        TaskEditKey::CreatedAt => {
            let normalized = normalize_rfc3339_input_for_preview("created_at", trimmed)
                .with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=created_at)",
                        needle
                    )
                })?;
            preview_task.created_at = normalized;
        }
        TaskEditKey::UpdatedAt => {
            let normalized = normalize_rfc3339_input_for_preview("updated_at", trimmed)
                .with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=updated_at)",
                        needle
                    )
                })?;
            preview_task.updated_at = normalized;
        }
        TaskEditKey::CompletedAt => {
            let normalized = normalize_rfc3339_input_for_preview("completed_at", trimmed)
                .with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=completed_at)",
                        needle
                    )
                })?;
            preview_task.completed_at = normalized;
        }
        TaskEditKey::StartedAt => {
            let normalized = normalize_rfc3339_input_for_preview("started_at", trimmed)
                .with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=started_at)",
                        needle
                    )
                })?;
            preview_task.started_at = normalized;
        }
        TaskEditKey::ScheduledStart => {
            let normalized = normalize_rfc3339_input_for_preview("scheduled_start", trimmed)
                .with_context(|| {
                    format!(
                        "Queue edit preview failed (task_id={}, field=scheduled_start)",
                        needle
                    )
                })?;
            preview_task.scheduled_start = normalized;
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

fn normalize_rfc3339_input_for_preview(label: &str, value: &str) -> Result<Option<String>> {
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

fn format_field_value(task: &Task, key: TaskEditKey) -> String {
    // Use semicolon separator for Evidence, Plan, Notes (longer text items)
    // Use comma separator for other list fields
    let sep = match key {
        TaskEditKey::Evidence | TaskEditKey::Plan | TaskEditKey::Notes => "; ",
        _ => ", ",
    };
    key.format_value(task, sep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
    use std::collections::HashMap;

    fn test_task() -> Task {
        Task {
            id: "RQ-0001".to_string(),
            title: "Test task".to_string(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            tags: vec!["rust".to_string(), "cli".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["step 1".to_string()],
            notes: vec!["note".to_string()],
            request: Some("test request".to_string()),
            created_at: Some("2026-01-20T12:00:00Z".to_string()),
            updated_at: Some("2026-01-20T12:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            agent: None,
            parent_id: None,
        }
    }

    fn test_queue() -> QueueFile {
        QueueFile {
            version: 1,
            tasks: vec![test_task()],
        }
    }

    #[test]
    fn preview_task_edit_shows_title_change() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let preview = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::Title,
            "New title",
            &now,
            "RQ",
            4,
            10,
        )
        .expect("preview should succeed");

        assert_eq!(preview.task_id, "RQ-0001");
        assert_eq!(preview.field, "title");
        assert_eq!(preview.old_value, "Test task");
        assert_eq!(preview.new_value, "New title");
    }

    #[test]
    fn preview_task_edit_shows_status_change() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let preview = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::Status,
            "doing",
            &now,
            "RQ",
            4,
            10,
        )
        .expect("preview should succeed");

        assert_eq!(preview.field, "status");
        assert_eq!(preview.old_value, "todo");
        assert_eq!(preview.new_value, "doing");
    }

    #[test]
    fn preview_task_edit_shows_priority_change() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let preview = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::Priority,
            "high",
            &now,
            "RQ",
            4,
            10,
        )
        .expect("preview should succeed");

        assert_eq!(preview.field, "priority");
        assert_eq!(preview.old_value, "medium");
        assert_eq!(preview.new_value, "high");
    }

    #[test]
    fn preview_task_edit_shows_tags_change() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let preview = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::Tags,
            "bug, urgent",
            &now,
            "RQ",
            4,
            10,
        )
        .expect("preview should succeed");

        assert_eq!(preview.field, "tags");
        assert_eq!(preview.old_value, "rust, cli");
        assert_eq!(preview.new_value, "bug, urgent");
    }

    #[test]
    fn preview_task_edit_validates_empty_title() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let result = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::Title,
            "",
            &now,
            "RQ",
            4,
            10,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("title cannot be empty"));
    }

    #[test]
    fn preview_task_edit_fails_for_missing_task() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let result = preview_task_edit(
            &queue,
            None,
            "RQ-9999",
            TaskEditKey::Title,
            "New title",
            &now,
            "RQ",
            4,
            10,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
    }

    #[test]
    fn preview_task_edit_validates_invalid_status() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let result = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::Status,
            "invalid_status",
            &now,
            "RQ",
            4,
            10,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // The error message is wrapped in context, so we check for the context message
        assert!(
            err.contains("field=status"),
            "error should mention field=status: {}",
            err
        );
    }

    #[test]
    fn preview_task_edit_clears_request_with_empty_string() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let preview = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::Request,
            "",
            &now,
            "RQ",
            4,
            10,
        )
        .expect("preview should succeed");

        assert_eq!(preview.field, "request");
        assert_eq!(preview.old_value, "test request");
        assert_eq!(preview.new_value, "");
    }

    #[test]
    fn preview_task_edit_shows_custom_fields_change() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let preview = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::CustomFields,
            "severity=high, owner=ralph",
            &now,
            "RQ",
            4,
            10,
        )
        .expect("preview should succeed");

        assert_eq!(preview.field, "custom_fields");
        assert_eq!(preview.old_value, "");
        // HashMap iteration order is not deterministic, so check for content not exact order
        assert!(
            preview.new_value.contains("severity=high"),
            "new_value should contain severity=high: {}",
            preview.new_value
        );
        assert!(
            preview.new_value.contains("owner=ralph"),
            "new_value should contain owner=ralph: {}",
            preview.new_value
        );
    }

    #[test]
    fn task_edit_key_format_value_with_newline_separator() {
        let task = test_task();

        assert_eq!(TaskEditKey::Tags.format_value(&task, "\n"), "rust\ncli");
        assert_eq!(TaskEditKey::Scope.format_value(&task, "\n"), "crates/ralph");
        assert_eq!(TaskEditKey::Title.format_value(&task, "\n"), "Test task");
    }

    #[test]
    fn task_edit_key_format_value_with_comma_separator() {
        let task = test_task();

        assert_eq!(TaskEditKey::Tags.format_value(&task, ", "), "rust, cli");
        assert_eq!(TaskEditKey::DependsOn.format_value(&task, ", "), "");
    }

    #[test]
    fn task_edit_key_is_list_field_identifies_lists_correctly() {
        assert!(TaskEditKey::Tags.is_list_field());
        assert!(TaskEditKey::Scope.is_list_field());
        assert!(TaskEditKey::Evidence.is_list_field());
        assert!(TaskEditKey::Plan.is_list_field());
        assert!(TaskEditKey::Notes.is_list_field());
        assert!(TaskEditKey::DependsOn.is_list_field());
        assert!(TaskEditKey::Blocks.is_list_field());
        assert!(TaskEditKey::RelatesTo.is_list_field());

        assert!(!TaskEditKey::Title.is_list_field());
        assert!(!TaskEditKey::Status.is_list_field());
        assert!(!TaskEditKey::Priority.is_list_field());
        assert!(!TaskEditKey::Request.is_list_field());
        assert!(!TaskEditKey::Duplicates.is_list_field());
        assert!(!TaskEditKey::ScheduledStart.is_list_field());
    }

    #[test]
    fn preview_task_edit_invalid_priority_includes_canonical_parser_error() {
        let queue = test_queue();
        let now = "2026-01-21T12:00:00Z".to_string();

        let err = preview_task_edit(
            &queue,
            None,
            "RQ-0001",
            TaskEditKey::Priority,
            "nope",
            &now,
            "RQ",
            4,
            10,
        )
        .unwrap_err();

        let msg = err.to_string();

        // The outer context message should contain field=priority
        assert!(msg.contains("field=priority"), "err was: {msg}");

        // The canonical parser error should be in the error chain (source)
        let expected = "nope".parse::<TaskPriority>().unwrap_err().to_string();
        let found_canonical = err.chain().any(|e| e.to_string().contains(&expected));
        assert!(
            found_canonical,
            "canonical error not in chain. err was: {msg}, expected: {expected}"
        );
    }

    #[test]
    fn format_field_value_uses_contextual_separators() {
        let mut task = test_task();
        task.evidence = vec!["item1".to_string(), "item2".to_string()];
        task.plan = vec!["step1".to_string(), "step2".to_string()];

        // Evidence uses "; " separator
        assert_eq!(
            format_field_value(&task, TaskEditKey::Evidence),
            "item1; item2"
        );

        // Plan uses "; " separator
        assert_eq!(format_field_value(&task, TaskEditKey::Plan), "step1; step2");

        // Tags uses ", " separator
        assert_eq!(format_field_value(&task, TaskEditKey::Tags), "rust, cli");
    }

    #[test]
    fn preview_and_apply_cycle_status_in_the_same_order() {
        let now = "2026-01-21T12:00:00Z".to_string();

        // Start from the module's default test task status (currently Todo).
        let mut apply_queue = test_queue();

        // Cycle through all statuses once, comparing preview's computed next value
        // to apply's real mutation at each step.
        for _ in 0..5 {
            let preview = preview_task_edit(
                &apply_queue,
                None,
                "RQ-0001",
                TaskEditKey::Status,
                "", // empty => cycle
                &now,
                "RQ",
                4,
                10,
            )
            .expect("preview should succeed");

            apply_task_edit(
                &mut apply_queue,
                None,
                "RQ-0001",
                TaskEditKey::Status,
                "", // empty => cycle
                &now,
                "RQ",
                4,
                10,
            )
            .expect("apply should succeed");

            let applied = apply_queue.tasks[0].status.to_string();
            assert_eq!(preview.new_value, applied);
        }
    }

    #[test]
    fn preview_and_apply_invalid_status_share_canonical_parse_error() {
        let now = "2026-01-21T12:00:00Z".to_string();

        let preview_err = preview_task_edit(
            &test_queue(),
            None,
            "RQ-0001",
            TaskEditKey::Status,
            "paused",
            &now,
            "RQ",
            4,
            10,
        )
        .unwrap_err();

        let apply_err = {
            let mut q = test_queue();
            apply_task_edit(
                &mut q,
                None,
                "RQ-0001",
                TaskEditKey::Status,
                "paused",
                &now,
                "RQ",
                4,
                10,
            )
            .unwrap_err()
        };

        let expected =
            "Invalid status: 'paused'. Expected one of: draft, todo, doing, done, rejected.";

        assert!(
            preview_err.chain().any(|e| e.to_string() == expected),
            "preview should include canonical parser error in chain: {}",
            preview_err
        );
        assert!(
            apply_err.chain().any(|e| e.to_string() == expected),
            "apply should include canonical parser error in chain: {}",
            apply_err
        );
    }
}
