//! `ralph task ...` command group: Clap types and handler.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::{agent, config, runner, task_cmd};

pub fn handle_task(cmd: TaskCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        TaskCommand::Build(args) => {
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                rp_on: args.rp_on,
                rp_off: args.rp_off,
            })?;
            let settings = runner::resolve_agent_settings(
                overrides.runner,
                overrides.model,
                overrides.reasoning_effort,
                None,
                &resolved.config.agent,
            )?;

            task_cmd::build_task(
                &resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: args.tags,
                    hint_scope: args.scope,
                    runner: settings.runner,
                    model: settings.model,
                    reasoning_effort: settings.reasoning_effort,
                    force,
                    repoprompt_required: agent::resolve_rp_required(
                        args.rp_on,
                        args.rp_off,
                        &resolved,
                    ),
                },
            )
        }
    }
}

#[derive(Args)]
#[command(
    about = "Create and build tasks from freeform requests",
    after_long_help = "Examples:\n  ralph task build \"Add tests for the new queue logic\"\n  ralph task build --runner opencode --model gpt-5.2 \"Fix CLI help strings\""
)]
pub struct TaskArgs {
    #[command(subcommand)]
    pub command: TaskCommand,
}

#[derive(Subcommand)]
pub enum TaskCommand {
    /// Build a new task from a natural language request.
    #[command(
        after_long_help = "Runner selection:\n  - Override runner/model/effort for this invocation using flags.\n  - Defaults come from config when flags are omitted.\n\nExamples:\n  ralph task build \"Add integration tests for run one\"\n  ralph task build --tags cli,rust \"Refactor queue parsing\"\n  ralph task build --scope crates/ralph \"Fix TUI rendering bug\"\n  ralph task build --runner opencode --model gpt-5.2 \"Add docs for OpenCode setup\"\n  ralph task build --runner gemini --model gemini-3-flash-preview \"Draft risk checklist\"\n  ralph task build --runner codex --model gpt-5.2-codex --effort high \"Fix queue validation\"\n  ralph task build --rp-on \"Audit error handling\"\n  ralph task build --rp-off \"Quick typo fix\"\n  echo \"Triage flaky CI\" | ralph task build --runner codex --model gpt-5.2-codex --effort medium"
    )]
    Build(TaskBuildArgs),
}

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
    #[arg(long)]
    pub effort: Option<String>,

    /// Force RepoPrompt required (must use context_builder).
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt not required.
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,
}
