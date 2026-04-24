//! CLI arguments for `ralph task decompose`.
//!
//! Purpose:
//! - CLI arguments for `ralph task decompose`.
//!
//! Responsibilities:
//! - Define clap arguments for decomposition previews and writes.
//! - Mirror runner override flags used by other runner-backed task commands.
//!
//! Not handled here:
//! - Source resolution or queue mutation.
//! - Planner execution or prompt rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Preview is the default mode; `--write` must be explicit for mutation.
//! - `source` may be a task ID or freeform request text.

use clap::Args;

use super::types::{TaskDecomposeChildPolicyArg, TaskDecomposeFormatArg, TaskStatusArg};
use crate::agent;

#[derive(Args, Clone, Debug)]
pub struct TaskDecomposeArgs {
    /// Task ID or freeform request text to decompose.
    /// If omitted, reads from stdin.
    #[arg(value_name = "SOURCE")]
    pub source: Vec<String>,

    /// Attach a new decomposition subtree under an existing parent task.
    #[arg(long, value_name = "TASK_ID")]
    pub attach_to: Option<String>,

    /// Maximum hierarchy depth to generate, including the root node.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=10))]
    pub max_depth: u8,

    /// Maximum number of children allowed for any single node.
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=25))]
    pub max_children: u8,

    /// Maximum total nodes allowed in the generated tree.
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=200))]
    pub max_nodes: u16,

    /// Status to assign to newly created tasks.
    #[arg(long, value_enum, default_value_t = TaskStatusArg::Draft)]
    pub status: TaskStatusArg,

    /// Child-tree handling when the effective parent already has children.
    #[arg(long, value_enum, default_value_t = TaskDecomposeChildPolicyArg::Fail)]
    pub child_policy: TaskDecomposeChildPolicyArg,

    /// Infer sibling dependencies from the planner output.
    #[arg(long)]
    pub with_dependencies: bool,

    /// Write the proposed decomposition into the queue.
    #[arg(long, conflicts_with = "preview")]
    pub write: bool,

    /// Explicitly request preview mode.
    #[arg(long, conflicts_with = "write")]
    pub preview: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = TaskDecomposeFormatArg::Text)]
    pub format: TaskDecomposeFormatArg,

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
}
