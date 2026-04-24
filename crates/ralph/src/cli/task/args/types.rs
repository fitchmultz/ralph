//! Shared types for task CLI arguments.
//!
//! Purpose:
//! - Shared types for task CLI arguments.
//!
//! Responsibilities:
//! - Define ValueEnum types for CLI argument parsing (BatchMode, TaskPriorityArg,
//!   TaskStatusArg, TaskEditFieldArg, and task decomposition enums).
//! - Provide conversions from CLI types to internal domain types.
//!
//! Not handled here:
//! - Args structs with clap derive macros (see specific command modules).
//! - Command execution logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All types must be Clone where needed for clap.
//! - Conversions to internal types are infallible and direct.

use clap::ValueEnum;

use crate::contracts::{TaskPriority, TaskStatus};
use crate::queue::TaskEditKey;

/// Batching mode for grouping related files in build-refactor.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
#[clap(rename_all = "snake_case")]
pub enum BatchMode {
    /// Group files in same directory with similar names (e.g., test files with source).
    #[default]
    Auto,
    /// Create individual task per file.
    Never,
    /// Group all files in same module/directory.
    Aggressive,
}

/// Task priority argument for CLI.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "snake_case")]
pub enum TaskPriorityArg {
    Critical,
    High,
    Medium,
    Low,
}

impl From<TaskPriorityArg> for TaskPriority {
    fn from(value: TaskPriorityArg) -> Self {
        match value {
            TaskPriorityArg::Critical => TaskPriority::Critical,
            TaskPriorityArg::High => TaskPriority::High,
            TaskPriorityArg::Medium => TaskPriority::Medium,
            TaskPriorityArg::Low => TaskPriority::Low,
        }
    }
}

/// Task status argument for CLI.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum TaskStatusArg {
    /// Task is a draft and not ready to run.
    Draft,
    /// Task is waiting to be started.
    Todo,
    /// Task is currently being worked on.
    Doing,
    /// Task is complete (terminal, archived).
    Done,
    /// Task was rejected (terminal, archived).
    Rejected,
}

impl From<TaskStatusArg> for TaskStatus {
    fn from(value: TaskStatusArg) -> Self {
        match value {
            TaskStatusArg::Draft => TaskStatus::Draft,
            TaskStatusArg::Todo => TaskStatus::Todo,
            TaskStatusArg::Doing => TaskStatus::Doing,
            TaskStatusArg::Done => TaskStatus::Done,
            TaskStatusArg::Rejected => TaskStatus::Rejected,
        }
    }
}

/// Output format for task decomposition.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[clap(rename_all = "snake_case")]
pub enum TaskDecomposeFormatArg {
    #[default]
    Text,
    Json,
}

/// Existing-child behavior for task decomposition writes.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[clap(rename_all = "snake_case")]
pub enum TaskDecomposeChildPolicyArg {
    #[default]
    Fail,
    Append,
    Replace,
}

/// Task edit field argument for CLI.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum TaskEditFieldArg {
    Title,
    Description,
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

impl TaskEditFieldArg {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskEditFieldArg::Title => "title",
            TaskEditFieldArg::Description => "description",
            TaskEditFieldArg::Status => "status",
            TaskEditFieldArg::Priority => "priority",
            TaskEditFieldArg::Tags => "tags",
            TaskEditFieldArg::Scope => "scope",
            TaskEditFieldArg::Evidence => "evidence",
            TaskEditFieldArg::Plan => "plan",
            TaskEditFieldArg::Notes => "notes",
            TaskEditFieldArg::Request => "request",
            TaskEditFieldArg::DependsOn => "depends_on",
            TaskEditFieldArg::Blocks => "blocks",
            TaskEditFieldArg::RelatesTo => "relates_to",
            TaskEditFieldArg::Duplicates => "duplicates",
            TaskEditFieldArg::CustomFields => "custom_fields",
            TaskEditFieldArg::Agent => "agent",
            TaskEditFieldArg::CreatedAt => "created_at",
            TaskEditFieldArg::UpdatedAt => "updated_at",
            TaskEditFieldArg::CompletedAt => "completed_at",
            TaskEditFieldArg::StartedAt => "started_at",
            TaskEditFieldArg::ScheduledStart => "scheduled_start",
            TaskEditFieldArg::EstimatedMinutes => "estimated_minutes",
            TaskEditFieldArg::ActualMinutes => "actual_minutes",
        }
    }
}

impl From<TaskEditFieldArg> for TaskEditKey {
    fn from(value: TaskEditFieldArg) -> Self {
        match value {
            TaskEditFieldArg::Title => TaskEditKey::Title,
            TaskEditFieldArg::Description => TaskEditKey::Description,
            TaskEditFieldArg::Status => TaskEditKey::Status,
            TaskEditFieldArg::Priority => TaskEditKey::Priority,
            TaskEditFieldArg::Tags => TaskEditKey::Tags,
            TaskEditFieldArg::Scope => TaskEditKey::Scope,
            TaskEditFieldArg::Evidence => TaskEditKey::Evidence,
            TaskEditFieldArg::Plan => TaskEditKey::Plan,
            TaskEditFieldArg::Notes => TaskEditKey::Notes,
            TaskEditFieldArg::Request => TaskEditKey::Request,
            TaskEditFieldArg::DependsOn => TaskEditKey::DependsOn,
            TaskEditFieldArg::Blocks => TaskEditKey::Blocks,
            TaskEditFieldArg::RelatesTo => TaskEditKey::RelatesTo,
            TaskEditFieldArg::Duplicates => TaskEditKey::Duplicates,
            TaskEditFieldArg::CustomFields => TaskEditKey::CustomFields,
            TaskEditFieldArg::Agent => TaskEditKey::Agent,
            TaskEditFieldArg::CreatedAt => TaskEditKey::CreatedAt,
            TaskEditFieldArg::UpdatedAt => TaskEditKey::UpdatedAt,
            TaskEditFieldArg::CompletedAt => TaskEditKey::CompletedAt,
            TaskEditFieldArg::StartedAt => TaskEditKey::StartedAt,
            TaskEditFieldArg::ScheduledStart => TaskEditKey::ScheduledStart,
            TaskEditFieldArg::EstimatedMinutes => TaskEditKey::EstimatedMinutes,
            TaskEditFieldArg::ActualMinutes => TaskEditKey::ActualMinutes,
        }
    }
}
