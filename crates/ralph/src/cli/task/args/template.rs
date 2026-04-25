//! CLI arguments for task template commands.
//!
//! Purpose:
//! - CLI arguments for task template commands.
//!
//! Responsibilities:
//! - Define Args structs for template commands and from-template commands.
//! - Define TaskTemplateCommand and TaskFromCommand enums.
//!
//! Not handled here:
//! - Command execution (see template and from_template handlers).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All types must be Clone where needed for clap.

use clap::{Args, Subcommand};

use crate::agent;

#[derive(Args)]
pub struct TaskTemplateArgs {
    #[command(subcommand)]
    pub command: TaskTemplateCommand,
}

/// Arguments for `ralph task from` command (parent of template subcommand).
#[derive(Args)]
pub struct TaskFromArgs {
    #[command(subcommand)]
    pub command: TaskFromCommand,
}

/// Subcommands for `ralph task from`
#[derive(Subcommand)]
pub enum TaskFromCommand {
    /// Build a task from a template with variable substitution.
    ///
    /// This is a convenience command that combines template selection,
    /// variable substitution, and task creation in one step.
    #[command(after_long_help = "Examples:
  ralph task from template bug --title \"Fix login timeout\"
  ralph task from template feature --title \"Add dark mode\" --set target=src/ui/theme.rs
  ralph task from template add-tests --title \"Test auth module\" --set target=src/auth/mod.rs
  ralph task from template refactor-performance --title \"Optimize parser\" --set target=src/parser/
  ralph task from template custom-template --title \"Custom task\" --set component=auth

Template variables:
  {{target}}  - Target file/path (set via --set target=... or positional TARGET)
  {{module}}  - Derived module name (e.g., src/cli/task.rs -> cli::task)
  {{file}}    - Filename only (e.g., task.rs)
  {{branch}}  - Current git branch name

Use 'ralph task template list' to see available templates.
Use 'ralph task template show <name>' to view template details.")]
    Template(TaskFromTemplateArgs),
}

/// Arguments for `ralph task from template` command.
#[derive(Args)]
pub struct TaskFromTemplateArgs {
    /// Template name (e.g., bug, feature, refactor, add-tests)
    pub template: String,

    /// Task title (required)
    #[arg(long = "title")]
    pub title: String,

    /// Set template variable (repeatable, format: VAR=value)
    /// Supported: target=PATH, component=NAME, or any custom variable used in the template
    #[arg(long = "set", value_name = "VAR=VALUE")]
    pub variables: Vec<String>,

    /// Additional tags to add (comma-separated)
    #[arg(short, long)]
    pub tags: Option<String>,

    /// Runner to use
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use
    #[arg(long)]
    pub model: Option<String>,

    /// Reasoning effort (Codex and Pi only)
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off)
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,

    /// Fail on unknown template variables
    #[arg(long)]
    pub strict_templates: bool,

    /// Preview task without adding to queue
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum TaskTemplateCommand {
    /// List available task templates
    List,
    /// Show template details
    Show(TaskTemplateShowArgs),
    /// Build a task from a template
    Build(TaskTemplateBuildArgs),
}

#[derive(Args)]
pub struct TaskTemplateShowArgs {
    /// Template name (e.g., "bug", "feature")
    pub name: String,
}

#[derive(Args)]
pub struct TaskTemplateBuildArgs {
    /// Template name
    pub template: String,

    /// Target file/path for template variable substitution ({{target}}, {{module}}, {{file}}).
    /// Used to auto-fill template variables with context from the specified path.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Task title/request
    pub request: Vec<String>,

    /// Additional tags to merge
    #[arg(short, long)]
    pub tags: Option<String>,

    /// Additional scope to merge
    #[arg(short, long)]
    pub scope: Option<String>,

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
