//! Prompt CLI argument definitions.
//!
//! Purpose:
//! - Prompt CLI argument definitions.
//!
//! Responsibilities:
//! - Define clap structures and enum routing for `ralph prompt ...`.
//! - Validate phase parsing for worker prompt previews.
//!
//! Not handled here:
//! - Prompt execution logic.
//! - Resolved-config based routing decisions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Phase parsing accepts only 1, 2, or 3.

use clap::{Args, Subcommand};

use crate::{agent, promptflow};

pub fn parse_phase(input: &str) -> anyhow::Result<promptflow::RunPhase> {
    match input {
        "1" => Ok(promptflow::RunPhase::Phase1),
        "2" => Ok(promptflow::RunPhase::Phase2),
        "3" => Ok(promptflow::RunPhase::Phase3),
        _ => anyhow::bail!("invalid phase '{input}', expected 1, 2, or 3"),
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
    Worker(PromptWorkerArgs),
    Scan(PromptScanArgs),
    TaskBuilder(PromptTaskBuilderArgs),
    List,
    Show(PromptShowArgs),
    Export(PromptExportArgs),
    Sync(PromptSyncArgs),
    Diff(PromptDiffArgs),
}

#[derive(Args)]
pub struct PromptWorkerArgs {
    #[arg(long, conflicts_with = "phase")]
    pub single: bool,
    #[arg(long, value_parser = parse_phase)]
    pub phase: Option<promptflow::RunPhase>,
    #[arg(long)]
    pub task_id: Option<String>,
    #[arg(long)]
    pub plan_file: Option<std::path::PathBuf>,
    #[arg(long)]
    pub plan_text: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub iterations: u8,
    #[arg(long, default_value_t = 1)]
    pub iteration_index: u8,
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,
    #[arg(long)]
    pub explain: bool,
}

#[derive(Args)]
pub struct PromptScanArgs {
    #[arg(long, default_value = "")]
    pub focus: String,
    #[arg(short = 'm', long, value_enum, default_value_t = super::super::scan::ScanMode::Maintenance)]
    pub mode: super::super::scan::ScanMode,
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,
    #[arg(long)]
    pub explain: bool,
}

#[derive(Args)]
pub struct PromptTaskBuilderArgs {
    #[arg(long)]
    pub request: Option<String>,
    #[arg(long, default_value = "")]
    pub tags: String,
    #[arg(long, default_value = "")]
    pub scope: String,
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,
    #[arg(long)]
    pub explain: bool,
}

#[derive(Args)]
pub struct PromptShowArgs {
    pub name: String,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Args)]
pub struct PromptExportArgs {
    pub name: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct PromptSyncArgs {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct PromptDiffArgs {
    pub name: String,
}
