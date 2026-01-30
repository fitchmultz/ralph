//! `ralph prompt ...` command group: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap arguments for prompt preview commands.
//! - Render worker/scan/task-builder prompts for debugging or inspection.
//! - List, show, export, and sync prompt templates.
//!
//! Not handled here:
//! - Queue persistence or task status updates.
//! - Runner execution or model invocation.
//! - Config file parsing beyond resolved config access.
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - RepoPrompt selection maps to plan/tool injection consistently.
//! - Prompt templates are managed via the prompts_internal module.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::{
    agent, commands::prompt as prompt_cmd, commands::task as task_cmd, config, promptflow,
};

pub fn handle_prompt(args: PromptArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        PromptCommand::Worker(p) => {
            let repoprompt_flags = agent::resolve_repoprompt_flags(p.repo_prompt, &resolved);

            let mode = if p.single {
                prompt_cmd::WorkerMode::Single
            } else if let Some(phase) = p.phase {
                match phase {
                    promptflow::RunPhase::Phase1 => prompt_cmd::WorkerMode::Phase1,
                    promptflow::RunPhase::Phase2 => prompt_cmd::WorkerMode::Phase2,
                    promptflow::RunPhase::Phase3 => prompt_cmd::WorkerMode::Phase3,
                }
            } else {
                // Default behavior: match runtime behavior as closely as possible.
                // If multi-phase planning is enabled, default to showing Phase 1 prompt (first prompt in the sequence).
                // Otherwise default to single-phase.
                if resolved.config.agent.phases.unwrap_or(2) > 1 {
                    prompt_cmd::WorkerMode::Phase1
                } else {
                    prompt_cmd::WorkerMode::Single
                }
            };

            let prompt = prompt_cmd::build_worker_prompt(
                &resolved,
                prompt_cmd::WorkerPromptOptions {
                    task_id: p.task_id,
                    mode,
                    repoprompt_plan_required: repoprompt_flags.plan_required,
                    repoprompt_tool_injection: repoprompt_flags.tool_injection,
                    iterations: p.iterations,
                    iteration_index: p.iteration_index,
                    plan_file: p.plan_file,
                    plan_text: p.plan_text,
                    explain: p.explain,
                },
            )?;
            print!("{prompt}");
        }
        PromptCommand::Scan(p) => {
            let rp_required = agent::resolve_rp_required(p.repo_prompt, &resolved);
            let prompt = prompt_cmd::build_scan_prompt(
                &resolved,
                prompt_cmd::ScanPromptOptions {
                    focus: p.focus,
                    mode: p.mode,
                    repoprompt_tool_injection: rp_required,
                    explain: p.explain,
                },
            )?;
            print!("{prompt}");
        }
        PromptCommand::TaskBuilder(p) => {
            let rp_required = agent::resolve_rp_required(p.repo_prompt, &resolved);

            // For convenience, allow stdin usage like `task` does.
            let request = if let Some(r) = p.request {
                r
            } else {
                // Re-use existing behavior to keep semantics consistent.
                task_cmd::read_request_from_args_or_stdin(&[])? // will read stdin if piped
            };

            let prompt = prompt_cmd::build_task_builder_prompt(
                &resolved,
                prompt_cmd::TaskBuilderPromptOptions {
                    request,
                    hint_tags: p.tags,
                    hint_scope: p.scope,
                    repoprompt_tool_injection: rp_required,
                    explain: p.explain,
                },
            )?;
            print!("{prompt}");
        }
        PromptCommand::List => {
            prompt_cmd::list_prompts(&resolved.repo_root)?;
        }
        PromptCommand::Show(p) => {
            prompt_cmd::show_prompt(&resolved.repo_root, &p.name, p.raw)?;
        }
        PromptCommand::Export(p) => {
            prompt_cmd::export_prompts(&resolved.repo_root, p.name.as_deref(), p.force)?;
        }
        PromptCommand::Sync(p) => {
            prompt_cmd::sync_prompts(&resolved.repo_root, p.dry_run, p.force)?;
        }
        PromptCommand::Diff(p) => {
            prompt_cmd::diff_prompt(&resolved.repo_root, &p.name)?;
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
    about = "Manage and inspect prompt templates",
    after_long_help = "Commands to view, export, and sync prompt templates.\n\nPreview compiled prompts (what the agent sees):\n  ralph prompt worker --phase 1 --repo-prompt plan\n  ralph prompt worker --single\n  ralph prompt scan --focus \"risk audit\" --repo-prompt off\n  ralph prompt task-builder --request \"Add tests\"\n\nList and view raw templates:\n  ralph prompt list\n  ralph prompt show worker --raw\n  ralph prompt diff worker\n\nExport and sync templates:\n  ralph prompt export --all\n  ralph prompt export worker\n  ralph prompt sync --dry-run\n  ralph prompt sync --force\n"
)]
pub struct PromptArgs {
    #[command(subcommand)]
    pub command: PromptCommand,
}

#[derive(Subcommand)]
pub enum PromptCommand {
    /// Render the worker prompt (single-phase or phase 1/2/3).
    Worker(PromptWorkerArgs),
    /// Render the scan prompt.
    Scan(PromptScanArgs),
    /// Render the task-builder prompt.
    TaskBuilder(PromptTaskBuilderArgs),
    /// List all available prompt templates.
    List,
    /// Show a specific prompt template (raw embedded or effective).
    Show(PromptShowArgs),
    /// Export embedded prompts to .ralph/prompts/ for customization.
    Export(PromptExportArgs),
    /// Sync exported prompts with embedded defaults.
    Sync(PromptSyncArgs),
    /// Show diff between user override and embedded default.
    Diff(PromptDiffArgs),
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

    /// Simulate total iteration count for prompt preview.
    #[arg(long, default_value_t = 1)]
    pub iterations: u8,

    /// Simulate which iteration index to preview (1-based).
    #[arg(long, default_value_t = 1)]
    pub iteration_index: u8,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    /// Print a header explaining what was selected (mode, sources, flags).
    #[arg(long)]
    pub explain: bool,
}

#[derive(Args)]
pub struct PromptScanArgs {
    /// Optional scan focus prompt.
    #[arg(long, default_value = "")]
    pub focus: String,

    /// Scan mode: maintenance (default) for code hygiene and bug finding,
    /// innovation for feature discovery and enhancement opportunities.
    #[arg(short = 'm', long, value_enum, default_value_t = super::scan::ScanMode::Maintenance)]
    pub mode: super::scan::ScanMode,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

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

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    /// Print a header explaining what was selected (sources, flags).
    #[arg(long)]
    pub explain: bool,
}

#[derive(Args)]
pub struct PromptShowArgs {
    /// Template name (e.g., worker, worker_phase1, scan).
    pub name: String,

    /// Show raw embedded content instead of effective (with override).
    #[arg(long)]
    pub raw: bool,
}

#[derive(Args)]
pub struct PromptExportArgs {
    /// Template name to export (e.g., worker). If omitted and --all not set, errors.
    pub name: Option<String>,

    /// Export all templates.
    #[arg(long)]
    pub all: bool,

    /// Overwrite existing files.
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct PromptSyncArgs {
    /// Preview changes without applying.
    #[arg(long)]
    pub dry_run: bool,

    /// Overwrite user modifications without prompting.
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct PromptDiffArgs {
    /// Template name to diff (e.g., worker).
    pub name: String,
}
