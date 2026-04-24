//! CLI arguments for runner management commands.
//!
//! Purpose:
//! - CLI arguments for runner management commands.
//!
//! Responsibilities:
//! - Define RunnerArgs and RunnerCommand enums for Clap.
//! - Delegate command execution to commands/runner/ module.
//!
//! Not handled here:
//! - Capability data retrieval (see commands/runner/capabilities.rs).
//! - Binary detection logic (see commands/runner/detection.rs).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

/// Arguments for the `ralph runner` command.
#[derive(Args)]
pub struct RunnerArgs {
    #[command(subcommand)]
    pub command: RunnerCommand,
}

#[derive(Subcommand)]
pub enum RunnerCommand {
    /// Show capabilities for a specific runner (models, features, binary status).
    Capabilities(CapabilitiesArgs),
    /// List all available runners with brief descriptions.
    List(ListArgs),
}

/// Output format for runner commands.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum RunnerFormat {
    /// Human-readable text output (default).
    #[default]
    Text,
    /// Machine-readable JSON output for scripting/CI.
    Json,
}

#[derive(Args)]
pub struct CapabilitiesArgs {
    /// Runner to inspect (codex, opencode, gemini, claude, cursor, kimi, pi, or plugin id).
    pub runner: String,

    /// Output format (text or json).
    #[arg(long, value_enum, default_value = "text")]
    pub format: RunnerFormat,
}

#[derive(Args)]
pub struct ListArgs {
    /// Output format (text or json).
    #[arg(long, value_enum, default_value = "text")]
    pub format: RunnerFormat,
}

pub fn handle_runner_capabilities(args: CapabilitiesArgs) -> Result<()> {
    crate::commands::runner::capabilities::handle_capabilities(&args.runner, args.format)
}

pub fn handle_runner_list(args: ListArgs) -> Result<()> {
    crate::commands::runner::list::handle_list(args.format)
}
