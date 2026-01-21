//! `ralph prompt ...` command group: Clap types and handler.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::{agent, config, promptflow};

pub fn handle_prompt(args: PromptArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        PromptCommand::Worker(p) => {
            let rp_required = agent::resolve_rp_required(p.rp_on, p.rp_off, &resolved);

            let mode = if p.single {
                crate::prompt_cmd::WorkerMode::Single
            } else if let Some(phase) = p.phase {
                match phase {
                    promptflow::RunPhase::Phase1 => crate::prompt_cmd::WorkerMode::Phase1,
                    promptflow::RunPhase::Phase2 => crate::prompt_cmd::WorkerMode::Phase2,
                    promptflow::RunPhase::Phase3 => crate::prompt_cmd::WorkerMode::Phase3,
                }
            } else {
                // Default behavior: match runtime behavior as closely as possible.
                // If multi-phase planning is enabled, default to showing Phase 1 prompt (first prompt in the sequence).
                // Otherwise default to single-phase.
                if resolved.config.agent.phases.unwrap_or(2) > 1 {
                    crate::prompt_cmd::WorkerMode::Phase1
                } else {
                    crate::prompt_cmd::WorkerMode::Single
                }
            };

            let prompt = crate::prompt_cmd::build_worker_prompt(
                &resolved,
                crate::prompt_cmd::WorkerPromptOptions {
                    task_id: p.task_id,
                    mode,
                    repoprompt_required: rp_required,
                    plan_file: p.plan_file,
                    plan_text: p.plan_text,
                    explain: p.explain,
                },
            )?;
            print!("{prompt}");
        }
        PromptCommand::Scan(p) => {
            let rp_required = agent::resolve_rp_required(p.rp_on, p.rp_off, &resolved);
            let prompt = crate::prompt_cmd::build_scan_prompt(
                &resolved,
                crate::prompt_cmd::ScanPromptOptions {
                    focus: p.focus,
                    repoprompt_required: rp_required,
                    explain: p.explain,
                },
            )?;
            print!("{prompt}");
        }
        PromptCommand::TaskBuilder(p) => {
            let rp_required = agent::resolve_rp_required(p.rp_on, p.rp_off, &resolved);

            // For convenience, allow stdin usage like `task` does.
            let request = if let Some(r) = p.request {
                r
            } else {
                // Re-use existing behavior to keep semantics consistent.
                crate::task_cmd::read_request_from_args_or_stdin(&[])? // will read stdin if piped
            };

            let prompt = crate::prompt_cmd::build_task_builder_prompt(
                &resolved,
                crate::prompt_cmd::TaskBuilderPromptOptions {
                    request,
                    hint_tags: p.tags,
                    hint_scope: p.scope,
                    repoprompt_required: rp_required,
                    explain: p.explain,
                },
            )?;
            print!("{prompt}");
        }
    }

    Ok(())
}

fn parse_phase(s: &str) -> Result<promptflow::RunPhase, String> {
    match s {
        "1" => Ok(promptflow::RunPhase::Phase1),
        "2" => Ok(promptflow::RunPhase::Phase2),
        "3" => Ok(promptflow::RunPhase::Phase3),
        _ => Err(format!("invalid phase '{}', expected 1, 2, or 3", s)),
    }
}

#[derive(Args)]
#[command(
    about = "Render and print compiled prompts (preview what the agent will see)",
    after_long_help = "This command prints the final compiled prompt after:\n  - loading embedded or overridden templates\n  - expanding config/env variables\n  - injecting project-type guidance\n  - applying phase wrappers and RepoPrompt requirements\n\nExamples:\n  ralph prompt worker --phase 1 --rp-on\n  ralph prompt worker --single\n  ralph prompt scan --focus \"risk audit\" --rp-off\n  ralph prompt task-builder --request \"Add tests\" --tags rust --scope crates/ralph\n"
)]
pub struct PromptArgs {
    #[command(subcommand)]
    pub command: PromptCommand,
}

#[derive(Subcommand)]
pub enum PromptCommand {
    /// Render the worker prompt (single-phase or phase 1/2).
    Worker(PromptWorkerArgs),
    /// Render the scan prompt.
    Scan(PromptScanArgs),
    /// Render the task-builder prompt.
    TaskBuilder(PromptTaskBuilderArgs),
}

#[derive(Args)]
pub struct PromptWorkerArgs {
    /// Force worker single-phase prompt (plan+implement in one prompt) even if two-pass is enabled.
    #[arg(long, conflicts_with = "phase")]
    pub single: bool,

    /// Force a specific worker phase (1=Plan, 2=Implement).
    #[arg(long, value_parser = parse_phase)]
    pub phase: Option<promptflow::RunPhase>,

    /// Task id to use for status-update instructions (defaults to first todo task).
    #[arg(long)]
    pub task_id: Option<String>,

    /// For phase 2: path to a plan file to embed.
    #[arg(long)]
    pub plan_file: Option<std::path::PathBuf>,

    /// For phase 2: inline plan text (takes precedence over --plan-file and cache).
    #[arg(long)]
    pub plan_text: Option<String>,

    /// Force RepoPrompt required.
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt not required.
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,

    /// Print a header explaining what was selected (mode, sources, flags).
    #[arg(long)]
    pub explain: bool,
}

#[derive(Args)]
pub struct PromptScanArgs {
    /// Optional scan focus prompt.
    #[arg(long, default_value = "")]
    pub focus: String,

    /// Force RepoPrompt required.
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt not required.
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,

    /// Print a header explaining what was selected (sources, flags).
    #[arg(long)]
    pub explain: bool,
}

#[derive(Args)]
pub struct PromptTaskBuilderArgs {
    /// Freeform request text; if omitted, reads from stdin.
    #[arg(long)]
    pub request: Option<String>,

    /// Optional hint tags (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    pub tags: String,

    /// Optional hint scope (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    pub scope: String,

    /// Force RepoPrompt required.
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt not required.
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,

    /// Print a header explaining what was selected (sources, flags).
    #[arg(long)]
    pub explain: bool,
}
