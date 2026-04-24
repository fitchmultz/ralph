//! CLI arguments for task relationship commands.
//!
//! Purpose:
//! - CLI arguments for task relationship commands.
//!
//! Responsibilities:
//! - Define Args structs for clone, split, children, parent, relate, blocks, and mark-duplicate commands.
//! - Define TaskRelationFormat enum for output formatting.
//!
//! Not handled here:
//! - Command execution (see clone, split, children, parent, and relations handlers).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All types must be Clone where needed for clap.

use clap::Args;

use crate::cli::task::args::types::TaskStatusArg;

/// Output format for task hierarchy commands (children, parent).
#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum TaskRelationFormat {
    Compact,
    Long,
    Json,
}

#[derive(Args)]
pub struct TaskCloneArgs {
    /// Source task ID to clone.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Status for the cloned task (default: draft).
    #[arg(long, value_enum)]
    pub status: Option<TaskStatusArg>,

    /// Prefix to add to the cloned task title.
    #[arg(long)]
    pub title_prefix: Option<String>,

    /// Preview the clone without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task split RQ-0001\n  ralph task split --number 3 RQ-0001\n  ralph task split --status todo --number 2 RQ-0001\n  ralph task split --distribute-plan RQ-0001"
)]
pub struct TaskSplitArgs {
    /// Task ID to split.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Number of child tasks to create (default: 2, minimum: 2).
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

    /// Preview the split without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
#[command(
    about = "List child tasks (parent_id == TASK_ID)",
    after_long_help = "Examples:\n ralph task children RQ-0001\n ralph task children RQ-0001 --recursive\n ralph task children RQ-0001 --include-done\n ralph task children RQ-0001 --format json"
)]
pub struct TaskChildrenArgs {
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    #[arg(long)]
    pub include_done: bool,

    #[arg(long)]
    pub recursive: bool,

    #[arg(long, value_enum, default_value_t = TaskRelationFormat::Compact)]
    pub format: TaskRelationFormat,
}

#[derive(Args)]
#[command(
    about = "Show a task's parent (parent_id)",
    after_long_help = "Examples:\n ralph task parent RQ-0002\n ralph task parent RQ-0002 --include-done\n ralph task parent RQ-0002 --format json"
)]
pub struct TaskParentArgs {
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    #[arg(long)]
    pub include_done: bool,

    #[arg(long, value_enum, default_value_t = TaskRelationFormat::Compact)]
    pub format: TaskRelationFormat,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task relate RQ-0001 blocks RQ-0002\n  ralph task relate RQ-0001 relates_to RQ-0003\n  ralph task relate RQ-0001 duplicates RQ-0004"
)]
pub struct TaskRelateArgs {
    /// Source task ID.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Relationship type (blocks, relates_to, duplicates).
    #[arg(value_name = "RELATION")]
    pub relation: String,

    /// Target task ID.
    #[arg(value_name = "OTHER_TASK_ID")]
    pub other_task_id: String,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task blocks RQ-0001 RQ-0002\n  ralph task blocks RQ-0001 RQ-0002 RQ-0003"
)]
pub struct TaskBlocksArgs {
    /// Task that does the blocking.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Task(s) being blocked.
    #[arg(value_name = "BLOCKED_TASK_ID...")]
    pub blocked_task_ids: Vec<String>,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph task mark-duplicate RQ-0001 RQ-0002")]
pub struct TaskMarkDuplicateArgs {
    /// Task to mark as duplicate.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Original task this duplicates.
    #[arg(value_name = "ORIGINAL_TASK_ID")]
    pub original_task_id: String,
}
