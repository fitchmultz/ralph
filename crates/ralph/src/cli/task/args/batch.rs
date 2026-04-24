//! CLI arguments for task batch operations.
//!
//! Purpose:
//! - CLI arguments for task batch operations.
//!
//! Responsibilities:
//! - Define BatchSelectArgs for task selection and filtering.
//! - Define BatchOperation enum and all batch operation argument structs.
//! - Define TaskBatchArgs for the batch command.
//!
//! Not handled here:
//! - Command execution (see batch handler).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All types must be Clone where needed for clap flattening.

use clap::{Args, Subcommand};

use crate::cli::task::args::types::{TaskEditFieldArg, TaskPriorityArg, TaskStatusArg};

/// Shared task selection + filters for batch operations.
#[derive(Args, Clone, Debug, Default)]
pub struct BatchSelectArgs {
    /// Task IDs to target (conflicts with --tag-filter).
    #[arg(value_name = "TASK_ID...", conflicts_with = "tag_filter")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag (case-insensitive, repeatable; OR logic).
    #[arg(long, value_name = "TAG", conflicts_with = "task_ids")]
    pub tag_filter: Vec<String>,

    /// Filter selected tasks by status (repeatable; OR logic).
    #[arg(long, value_enum, value_name = "STATUS")]
    pub status_filter: Vec<TaskStatusArg>,

    /// Filter selected tasks by priority (repeatable; OR logic).
    #[arg(long, value_enum, value_name = "PRIORITY")]
    pub priority_filter: Vec<TaskPriorityArg>,

    /// Filter selected tasks by scope substring (repeatable; OR logic; case-insensitive).
    #[arg(long, value_name = "PATTERN")]
    pub scope_filter: Vec<String>,

    /// Filter selected tasks whose updated_at is older than this cutoff.
    /// Supported forms: "7d", "1w", "2026-01-01", RFC3339
    #[arg(long, value_name = "WHEN")]
    pub older_than: Option<String>,
}

/// Batch operation type.
#[derive(Subcommand)]
pub enum BatchOperation {
    /// Update status for multiple tasks.
    Status(BatchStatusArgs),
    /// Set a custom field on multiple tasks.
    Field(BatchFieldArgs),
    /// Edit any field on multiple tasks.
    Edit(BatchEditArgs),

    /// Delete multiple tasks from the active queue.
    Delete(BatchDeleteArgs),

    /// Archive terminal tasks (Done/Rejected) from active queue into done archive.
    Archive(BatchArchiveArgs),

    /// Clone multiple tasks.
    Clone(BatchCloneArgs),

    /// Split multiple tasks into child tasks.
    Split(BatchSplitArgs),

    /// Append plan items to multiple tasks.
    #[command(name = "plan-append")]
    PlanAppend(BatchPlanAppendArgs),

    /// Prepend plan items to multiple tasks.
    #[command(name = "plan-prepend")]
    PlanPrepend(BatchPlanPrependArgs),
}

/// Arguments for batch status operation.
#[derive(Args)]
pub struct BatchStatusArgs {
    /// New status.
    #[arg(value_enum)]
    pub status: TaskStatusArg,

    /// Optional note to append to all affected tasks.
    #[arg(long)]
    pub note: Option<String>,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch field operation.
#[derive(Args)]
pub struct BatchFieldArgs {
    /// Custom field key.
    pub key: String,

    /// Custom field value.
    pub value: String,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch edit operation.
#[derive(Args)]
pub struct BatchEditArgs {
    /// Task field to update.
    #[arg(value_enum)]
    pub field: TaskEditFieldArg,

    /// New field value.
    pub value: String,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch delete operation.
#[derive(Args)]
pub struct BatchDeleteArgs {
    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch archive operation.
#[derive(Args)]
pub struct BatchArchiveArgs {
    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch clone operation.
#[derive(Args)]
pub struct BatchCloneArgs {
    /// Status for the cloned tasks (default: draft).
    #[arg(long, value_enum)]
    pub status: Option<TaskStatusArg>,

    /// Prefix to add to the cloned task titles.
    #[arg(long)]
    pub title_prefix: Option<String>,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch split operation.
#[derive(Args)]
pub struct BatchSplitArgs {
    /// Number of child tasks to create per source task (default: 2, minimum: 2).
    #[arg(short = 'n', long, default_value = "2")]
    pub number: usize,

    /// Status for child tasks (default: draft).
    #[arg(long, value_enum)]
    pub status: Option<TaskStatusArg>,

    /// Prefix to add to child task titles.
    #[arg(long)]
    pub title_prefix: Option<String>,

    /// Distribute plan items across child tasks.
    #[arg(long)]
    pub distribute_plan: bool,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch plan-append operation.
#[derive(Args)]
pub struct BatchPlanAppendArgs {
    /// Plan items to append (repeatable).
    #[arg(long = "plan-item", value_name = "ITEM", required = true)]
    pub plan_items: Vec<String>,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch plan-prepend operation.
#[derive(Args)]
pub struct BatchPlanPrependArgs {
    /// Plan items to prepend (repeatable).
    #[arg(long = "plan-item", value_name = "ITEM", required = true)]
    pub plan_items: Vec<String>,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for the batch command.
#[derive(Args)]
pub struct TaskBatchArgs {
    /// Batch operation type.
    #[command(subcommand)]
    pub operation: BatchOperation,

    /// Preview changes without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,

    /// Continue on individual task failures (default: atomic/all-or-nothing).
    #[arg(long)]
    pub continue_on_error: bool,
}
