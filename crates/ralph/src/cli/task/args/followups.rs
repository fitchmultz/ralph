//! CLI arguments for task follow-up proposal application.
//!
//! Purpose:
//! - CLI arguments for task follow-up proposal application.
//!
//! Responsibilities:
//! - Define nested `ralph task followups ...` subcommands.
//! - Expose text vs JSON output selection for apply reports.
//! - Keep proposal-file path and source-task arguments typed for handlers.
//!
//! Not handled here:
//! - Proposal parsing, queue mutation, or persistence.
//! - Runner prompt guidance for when proposals should be created.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Missing `--input` means `.ralph/cache/followups/<TASK_ID>.json`.
//! - `--task` names the source task that authorized the proposal.

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskFollowupsFormatArg {
    Text,
    Json,
}

#[derive(Args, Clone)]
pub struct TaskFollowupsArgs {
    #[command(subcommand)]
    pub command: TaskFollowupsCommand,
}

#[derive(Subcommand, Clone)]
pub enum TaskFollowupsCommand {
    /// Apply a followups@v1 proposal into the queue.
    #[command(
        after_long_help = "Examples:\n ralph task followups apply --task RQ-0135\n ralph task followups apply --task RQ-0135 --dry-run\n ralph task followups apply --task RQ-0135 --input /tmp/followups.json --format json"
    )]
    Apply(TaskFollowupsApplyArgs),
}

#[derive(Args, Clone)]
pub struct TaskFollowupsApplyArgs {
    /// Source task that produced the follow-up proposal.
    #[arg(long, value_name = "TASK_ID")]
    pub task: String,

    /// Read the followups@v1 proposal from this path.
    ///
    /// When omitted, Ralph reads `.ralph/cache/followups/<TASK_ID>.json`.
    #[arg(long, value_name = "PATH")]
    pub input: Option<PathBuf>,

    /// Preview created tasks without saving queue changes or removing the proposal.
    #[arg(long)]
    pub dry_run: bool,

    /// Output format for the apply report.
    #[arg(long, value_enum, default_value_t = TaskFollowupsFormatArg::Text)]
    pub format: TaskFollowupsFormatArg,
}
