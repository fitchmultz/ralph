//! Clap argument definitions for `ralph run`.
//!
//! Purpose:
//! - Clap argument definitions for `ralph run`.
//!
//! Responsibilities:
//! - Define clap types for `run`, `run one`, `run loop`, `run resume`, and `run parallel`.
//!
//! Not handled here:
//! - Dispatch logic.
//! - Long-help text authoring.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Clap validation remains colocated with the flags it governs.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::help::{
    PARALLEL_AFTER_LONG_HELP, PARALLEL_RETRY_AFTER_LONG_HELP, PARALLEL_STATUS_AFTER_LONG_HELP,
    RESUME_AFTER_LONG_HELP, RUN_AFTER_LONG_HELP, RUN_LOOP_AFTER_LONG_HELP, RUN_ONE_AFTER_LONG_HELP,
};

#[derive(Args)]
#[command(
    about = "Run Ralph supervisor (executes queued tasks via codex/opencode/gemini/claude/cursor/kimi/pi)",
    after_long_help = RUN_AFTER_LONG_HELP
)]
pub struct RunArgs {
    #[command(subcommand)]
    pub command: RunCommand,
}

#[derive(Subcommand)]
pub enum RunCommand {
    #[command(about = "Resume an interrupted session from where it left off", after_long_help = RESUME_AFTER_LONG_HELP)]
    Resume(ResumeArgs),
    #[command(about = "Run exactly one task (the first todo in the configured queue file)", after_long_help = RUN_ONE_AFTER_LONG_HELP)]
    One(RunOneArgs),
    #[command(about = "Run tasks repeatedly until no todo remain (or --max-tasks is reached)", after_long_help = RUN_LOOP_AFTER_LONG_HELP)]
    Loop(RunLoopArgs),
    #[command(
        about = "Experimental: manage parallel mode operations",
        after_long_help = PARALLEL_AFTER_LONG_HELP,
        hide = true
    )]
    Parallel(ParallelArgs),
}

#[derive(Args)]
pub struct ResumeArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub debug: bool,
    #[arg(long)]
    pub non_interactive: bool,
    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

#[derive(Args)]
pub struct RunOneArgs {
    #[arg(long)]
    pub debug: bool,
    #[arg(long)]
    pub resume: bool,
    #[arg(long, value_name = "TASK_ID")]
    pub id: Option<String>,
    #[arg(long)]
    pub non_interactive: bool,
    #[arg(long, conflicts_with = "parallel_worker")]
    pub dry_run: bool,
    #[arg(
        long,
        hide = true,
        conflicts_with = "resume",
        requires_all = ["id", "coordinator_queue_path", "coordinator_done_path", "parallel_target_branch"]
    )]
    pub parallel_worker: bool,
    #[arg(
        long,
        hide = true,
        value_name = "PATH",
        requires_all = ["parallel_worker", "coordinator_done_path"]
    )]
    pub coordinator_queue_path: Option<PathBuf>,
    #[arg(
        long,
        hide = true,
        value_name = "PATH",
        requires_all = ["parallel_worker", "coordinator_queue_path"]
    )]
    pub coordinator_done_path: Option<PathBuf>,
    #[arg(
        long,
        hide = true,
        value_name = "BRANCH",
        requires_all = ["parallel_worker", "coordinator_queue_path", "coordinator_done_path"]
    )]
    pub parallel_target_branch: Option<String>,
    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

#[derive(Args)]
pub struct RunLoopArgs {
    #[arg(long, default_value_t = 0)]
    pub max_tasks: u32,
    #[arg(long)]
    pub debug: bool,
    #[arg(long)]
    pub resume: bool,
    #[arg(long)]
    pub non_interactive: bool,
    #[arg(long, conflicts_with = "parallel")]
    pub dry_run: bool,
    #[arg(
        long,
        value_parser = clap::value_parser!(u8).range(2..),
        num_args = 0..=1,
        default_missing_value = "2",
        value_name = "N",
    )]
    pub parallel: Option<u8>,
    #[arg(long, conflicts_with = "parallel")]
    pub wait_when_blocked: bool,
    #[arg(
        long,
        default_value_t = 1000,
        value_parser = clap::value_parser!(u64).range(50..),
        value_name = "MS"
    )]
    pub wait_poll_ms: u64,
    #[arg(long, default_value_t = 0, value_name = "SECONDS")]
    pub wait_timeout_seconds: u64,
    #[arg(long)]
    pub notify_when_unblocked: bool,
    #[arg(long, alias = "continuous", conflicts_with = "parallel")]
    pub wait_when_empty: bool,
    #[arg(
        long,
        default_value_t = 30_000,
        value_parser = clap::value_parser!(u64).range(50..),
        value_name = "MS"
    )]
    pub empty_poll_ms: u64,
    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

#[derive(Args)]
pub struct ParallelArgs {
    #[command(subcommand)]
    pub command: ParallelSubcommand,
}

#[derive(Subcommand)]
pub enum ParallelSubcommand {
    #[command(about = "Show status of parallel workers", after_long_help = PARALLEL_STATUS_AFTER_LONG_HELP)]
    Status(ParallelStatusArgs),
    #[command(about = "Retry a blocked or failed parallel worker", after_long_help = PARALLEL_RETRY_AFTER_LONG_HELP)]
    Retry(ParallelRetryArgs),
}

#[derive(Args)]
pub struct ParallelStatusArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct ParallelRetryArgs {
    #[arg(long, value_name = "TASK_ID", required = true)]
    pub task: String,
}
