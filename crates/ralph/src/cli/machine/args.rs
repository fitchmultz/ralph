//! Clap argument definitions for `ralph machine`.
//!
//! Purpose:
//! - Clap argument definitions for `ralph machine`.
//!
//! Responsibilities:
//! - Define the versioned machine-facing subcommands consumed by the macOS app.
//! - Keep machine Clap wiring separate from execution logic.
//! - Re-export request-shaping flags shared by the machine handlers.
//!
//! Not handled here:
//! - JSON document emission.
//! - Queue/task/run business logic.
//! - Machine contract schema types.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Machine commands stay non-human-facing and versioned.
//! - Subcommand shapes remain stable unless machine contract versions change.

use clap::Args;
use clap::Subcommand;

use crate::agent;

#[derive(Args)]
pub struct MachineArgs {
    #[command(subcommand)]
    pub command: MachineCommand,
}

#[derive(Subcommand)]
pub enum MachineCommand {
    System(MachineSystemArgs),
    Queue(MachineQueueArgs),
    Config(MachineConfigArgs),
    Workspace(MachineWorkspaceArgs),
    Task(MachineTaskArgs),
    Run(Box<MachineRunArgs>),
    Doctor(MachineDoctorArgs),
    CliSpec,
    Schema,
}

#[derive(Args)]
pub struct MachineSystemArgs {
    #[command(subcommand)]
    pub command: MachineSystemCommand,
}

#[derive(Subcommand)]
pub enum MachineSystemCommand {
    Info,
}

#[derive(Args)]
pub struct MachineQueueArgs {
    #[command(subcommand)]
    pub command: MachineQueueCommand,
}

#[derive(Subcommand)]
pub enum MachineQueueCommand {
    Read,
    Graph,
    Dashboard(MachineDashboardArgs),
    Validate,
    Repair(MachineQueueRepairArgs),
    Undo(MachineQueueUndoArgs),
    UnlockInspect,
}

#[derive(Args)]
pub struct MachineDashboardArgs {
    #[arg(long, default_value_t = 30)]
    pub days: u32,
}

#[derive(Args)]
pub struct MachineQueueRepairArgs {
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct MachineQueueUndoArgs {
    #[arg(long, short)]
    pub id: Option<String>,
    #[arg(long)]
    pub list: bool,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct MachineConfigArgs {
    #[command(subcommand)]
    pub command: MachineConfigCommand,
}

#[derive(Subcommand)]
pub enum MachineConfigCommand {
    Resolve,
}

#[derive(Args)]
pub struct MachineWorkspaceArgs {
    #[command(subcommand)]
    pub command: MachineWorkspaceCommand,
}

#[derive(Subcommand)]
pub enum MachineWorkspaceCommand {
    Overview,
}

#[derive(Args)]
pub struct MachineTaskArgs {
    #[command(subcommand)]
    pub command: MachineTaskCommand,
}

#[derive(Subcommand)]
pub enum MachineTaskCommand {
    Build(Box<MachineTaskBuildArgs>),
    Create(MachineTaskCreateArgs),
    Mutate(MachineTaskMutateArgs),
    Decompose(Box<MachineTaskDecomposeArgs>),
}

#[derive(Args)]
pub struct MachineTaskBuildArgs {
    #[arg(long, value_name = "PATH")]
    pub input: Option<String>,
    #[command(flatten)]
    pub agent: agent::AgentArgs,
}

#[derive(Args)]
pub struct MachineTaskCreateArgs {
    #[arg(long, value_name = "PATH")]
    pub input: Option<String>,
}

#[derive(Args)]
pub struct MachineTaskMutateArgs {
    #[arg(long, value_name = "PATH")]
    pub input: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct MachineTaskDecomposeArgs {
    pub source: Vec<String>,
    #[arg(long)]
    pub attach_to: Option<String>,
    #[arg(long, default_value_t = 3)]
    pub max_depth: u8,
    #[arg(long, default_value_t = 5)]
    pub max_children: u8,
    #[arg(long, default_value_t = 50)]
    pub max_nodes: u8,
    #[arg(long, default_value = "draft")]
    pub status: String,
    #[arg(long, default_value = "fail")]
    pub child_policy: String,
    #[arg(long)]
    pub with_dependencies: bool,
    #[arg(long)]
    pub write: bool,
    #[command(flatten)]
    pub agent: agent::AgentArgs,
}

#[derive(Args)]
pub struct MachineRunArgs {
    #[command(subcommand)]
    pub command: MachineRunCommand,
}

#[derive(Subcommand)]
pub enum MachineRunCommand {
    One(MachineRunOneArgs),
    Loop(MachineRunLoopArgs),
    Stop(MachineRunStopArgs),
    ParallelStatus,
}

#[derive(Args)]
pub struct MachineRunOneArgs {
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub resume: bool,
    #[command(flatten)]
    pub agent: agent::RunAgentArgs,
}

#[derive(Args)]
pub struct MachineRunLoopArgs {
    #[arg(long, default_value_t = 0)]
    pub max_tasks: u32,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub resume: bool,
    #[arg(
        long,
        value_parser = clap::value_parser!(u8).range(2..),
        num_args = 0..=1,
        default_missing_value = "2",
        value_name = "N",
    )]
    pub parallel: Option<u8>,
    #[command(flatten)]
    pub agent: agent::RunAgentArgs,
}

#[derive(Args)]
pub struct MachineRunStopArgs {
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct MachineDoctorArgs {
    #[command(subcommand)]
    pub command: MachineDoctorCommand,
}

#[derive(Subcommand)]
pub enum MachineDoctorCommand {
    Report,
}
