//! CLI arguments for task build commands.
//!
//! Purpose:
//! - CLI arguments for task build commands.
//!
//! Responsibilities:
//! - Define Args structs for TaskBuildArgs and TaskBuildRefactorArgs.
//!
//! Not handled here:
//! - Command execution (see build and refactor handlers).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All types must be Clone where needed for clap flattening.

use clap::Args;

use crate::agent;
use crate::cli::task::args::types::BatchMode;

#[derive(Args)]
pub struct TaskBuildArgs {
    /// Freeform request text; if omitted, reads from stdin.
    #[arg(value_name = "REQUEST")]
    pub request: Vec<String>,

    /// Optional hint tags (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    pub tags: String,

    /// Optional hint scope (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    pub scope: String,

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

    /// Template to use for pre-filling task fields (bug, feature, refactor, test, docs,
    /// add-tests, refactor-performance, fix-error-handling, add-docs, security-audit).
    #[arg(short = 't', long, value_name = "TEMPLATE")]
    pub template: Option<String>,

    /// Target file/path for template variable substitution ({{target}}, {{module}}, {{file}}).
    /// Used with --template to auto-fill template variables.
    #[arg(long, value_name = "PATH")]
    pub target: Option<String>,

    /// Estimated time to complete (e.g., "30m", "2h", "1h30m").
    /// Stored as minutes in the task's estimated_minutes field.
    #[arg(long, value_name = "DURATION")]
    pub estimate: Option<String>,

    /// Fail on unknown template variables (default: warn only).
    /// When enabled, template loading fails if the template contains unknown {{variables}}.
    /// When disabled (default), unknown variables are left as-is with a warning.
    #[arg(long)]
    pub strict_templates: bool,
}

#[derive(Args)]
#[command(after_long_help = "Examples:
 ralph task build refactor
 ralph task build refactor --threshold 700
 ralph task build refactor --path crates/ralph/src/cli
 ralph task build refactor --dry-run --threshold 500
 ralph task build refactor --batch never
 ralph task build refactor --tags urgent,technical-debt")]
pub struct TaskBuildRefactorArgs {
    /// LOC threshold for flagging files as "large" (default: 1000).
    /// Files exceeding ~1000 LOC are presumed mis-scoped per AGENTS.md.
    #[arg(long, default_value = "1000")]
    pub threshold: usize,

    /// Directory to scan for Rust files (default: current directory / repo root).
    #[arg(long)]
    pub path: Option<std::path::PathBuf>,

    /// Preview tasks without inserting into queue.
    #[arg(long)]
    pub dry_run: bool,

    /// Batching behavior for related files.
    /// - auto: Group files in same directory with similar names (default).
    /// - never: Create individual task per file.
    /// - aggressive: Group all files in same module.
    #[arg(long, value_enum, default_value = "auto")]
    pub batch: BatchMode,

    /// Additional tags to add to generated tasks (comma-separated).
    #[arg(long)]
    pub tags: Option<String>,

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

    /// Fail on unknown template variables (default: warn only).
    /// When enabled, template loading fails if the template contains unknown {{variables}}.
    /// When disabled (default), unknown variables are left as-is with a warning.
    #[arg(long)]
    pub strict_templates: bool,
}
