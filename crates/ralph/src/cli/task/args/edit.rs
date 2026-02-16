//! CLI arguments for task edit commands.
//!
//! Responsibilities:
//! - Define Args structs for field, edit, and update commands.
//!
//! Not handled here:
//! - Command execution (see edit handler).
//!
//! Invariants/assumptions:
//! - All types must be Clone where needed for clap.

use clap::Args;

use crate::agent;
use crate::cli::task::args::types::TaskEditFieldArg;

#[derive(Args)]
pub struct TaskFieldArgs {
    /// Custom field key (must not contain whitespace).
    pub key: String,

    /// Custom field value.
    pub value: String,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

#[derive(Args)]
pub struct TaskEditArgs {
    /// Task field to update.
    #[arg(value_enum)]
    pub field: TaskEditFieldArg,

    /// New field value (empty string clears optional fields).
    pub value: String,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,

    /// Preview changes without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct TaskUpdateArgs {
    /// Fields to update (comma-separated, default: all).
    ///
    /// Valid fields: scope, evidence, plan, notes, tags, depends_on
    #[arg(long, default_value = "")]
    pub fields: String,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,

    /// Task ID to update (omit to update all tasks).
    #[arg(value_name = "TASK_ID")]
    pub task_id: Option<String>,

    /// Preview changes without modifying the queue.
    ///
    /// For task update, this shows the prompt that would be sent to the runner.
    /// Actual changes depend on runner analysis of repository state.
    #[arg(long)]
    pub dry_run: bool,
}
