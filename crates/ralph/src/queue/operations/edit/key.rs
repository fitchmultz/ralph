//! Task edit key definitions.
//!
//! Responsibilities:
//! - Define the `TaskEditKey` enum representing editable task fields.
//! - Provide string parsing and formatting for task edit keys.
//!
//! Does not handle:
//! - Actual task editing logic (see `apply.rs` and `preview.rs`).
//! - Input validation beyond key parsing.
//!
//! Assumptions/invariants:
//! - TaskEditKey variants map 1:1 with Task struct fields.
//! - String representations use snake_case for consistency.

use crate::contracts::Task;
use anyhow::{Result, bail};

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
    Agent,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
    StartedAt,
    ScheduledStart,
    EstimatedMinutes,
    ActualMinutes,
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
            TaskEditKey::Agent => "agent",
            TaskEditKey::CreatedAt => "created_at",
            TaskEditKey::UpdatedAt => "updated_at",
            TaskEditKey::CompletedAt => "completed_at",
            TaskEditKey::StartedAt => "started_at",
            TaskEditKey::ScheduledStart => "scheduled_start",
            TaskEditKey::EstimatedMinutes => "estimated_minutes",
            TaskEditKey::ActualMinutes => "actual_minutes",
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
            TaskEditKey::Agent => task
                .agent
                .as_ref()
                .and_then(|agent| serde_json::to_string(agent).ok())
                .unwrap_or_default(),
            TaskEditKey::CreatedAt => task.created_at.clone().unwrap_or_default(),
            TaskEditKey::UpdatedAt => task.updated_at.clone().unwrap_or_default(),
            TaskEditKey::CompletedAt => task.completed_at.clone().unwrap_or_default(),
            TaskEditKey::StartedAt => task.started_at.clone().unwrap_or_default(),
            TaskEditKey::ScheduledStart => task.scheduled_start.clone().unwrap_or_default(),
            TaskEditKey::EstimatedMinutes => task
                .estimated_minutes
                .map(|m| m.to_string())
                .unwrap_or_default(),
            TaskEditKey::ActualMinutes => task
                .actual_minutes
                .map(|m| m.to_string())
                .unwrap_or_default(),
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
            "agent" => Ok(TaskEditKey::Agent),
            "created_at" => Ok(TaskEditKey::CreatedAt),
            "updated_at" => Ok(TaskEditKey::UpdatedAt),
            "completed_at" => Ok(TaskEditKey::CompletedAt),
            "started_at" => Ok(TaskEditKey::StartedAt),
            "scheduled_start" => Ok(TaskEditKey::ScheduledStart),
            "estimated_minutes" => Ok(TaskEditKey::EstimatedMinutes),
            "actual_minutes" => Ok(TaskEditKey::ActualMinutes),
            _ => bail!(
                "Unknown task field: '{}'. Expected one of: title, status, priority, tags, scope, evidence, plan, notes, request, depends_on, blocks, relates_to, duplicates, custom_fields, agent, created_at, updated_at, completed_at, started_at, scheduled_start, estimated_minutes, actual_minutes.",
                value
            ),
        }
    }
}
