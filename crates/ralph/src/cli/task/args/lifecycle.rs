//! CLI arguments for task lifecycle commands.
//!
//! Responsibilities:
//! - Define Args structs for show, ready, status, done, start, reject, and schedule commands.
//!
//! Not handled here:
//! - Command execution (see status, show, start, and schedule handlers).
//!
//! Invariants/assumptions:
//! - All types must be Clone where needed for clap.

use clap::Args;

use crate::cli::queue::QueueShowFormat;
use crate::cli::task::args::types::TaskStatusArg;

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n ralph task show RQ-0001\n ralph task show RQ-0001 --format compact"
)]
pub struct TaskShowArgs {
    /// Task ID to show.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueShowFormat::Json)]
    pub format: QueueShowFormat,
}

#[derive(Args)]
pub struct TaskReadyArgs {
    /// Optional note to append when marking ready.
    #[arg(long)]
    pub note: Option<String>,

    /// Draft task ID to promote.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Args)]
pub struct TaskStatusArgs {
    /// Optional note to append.
    #[arg(long)]
    pub note: Option<String>,

    /// New status.
    #[arg(value_enum)]
    pub status: TaskStatusArg,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

#[derive(Args)]
pub struct TaskDoneArgs {
    /// Notes to append (repeatable).
    #[arg(long)]
    pub note: Vec<String>,

    /// Task ID to complete.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Args)]
#[command(
    about = "Mark a task as started (sets started_at and moves to doing)",
    after_long_help = "Examples:\n ralph task start RQ-0001\n ralph task start --reset RQ-0001"
)]
pub struct TaskStartArgs {
    /// Task ID to start.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Reset started_at even if already set.
    #[arg(long)]
    pub reset: bool,
}

#[derive(Args)]
pub struct TaskRejectArgs {
    /// Notes to append (repeatable).
    #[arg(long)]
    pub note: Vec<String>,

    /// Task ID to reject.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task schedule RQ-0001 '2026-02-01T09:00:00Z'\n  ralph task schedule RQ-0001 'tomorrow 9am'\n  ralph task schedule RQ-0001 'in 2 hours'\n  ralph task schedule RQ-0001 'next monday'\n  ralph task schedule RQ-0001 --clear"
)]
pub struct TaskScheduleArgs {
    /// Task ID to schedule.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Timestamp or relative time expression (e.g., 'tomorrow 9am', 'in 2 hours').
    #[arg(value_name = "WHEN")]
    pub when: Option<String>,

    /// Clear the scheduled start time.
    #[arg(long, conflicts_with = "when")]
    pub clear: bool,
}
