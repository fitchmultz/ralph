//! `ralph daemon ...` command group for background service management.
//!
//! Responsibilities:
//! - Define clap structures for daemon commands and flags.
//! - Route daemon subcommands to the daemon command implementations.
//!
//! Not handled here:
//! - Daemon process management (see `crate::commands::daemon`).
//! - Queue execution logic (see `crate::commands::run`).
//!
//! Invariants/assumptions:
//! - Daemon is Unix-only (Windows uses different service mechanisms).
//! - Daemon uses a dedicated lock separate from the queue lock.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::{commands::daemon as daemon_cmd, config};

pub fn handle_daemon(cmd: DaemonCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        DaemonCommand::Start(args) => daemon_cmd::start(&resolved, args),
        DaemonCommand::Stop => daemon_cmd::stop(&resolved),
        DaemonCommand::Status => daemon_cmd::status(&resolved),
        DaemonCommand::Serve(args) => daemon_cmd::serve(&resolved, args),
    }
}

#[derive(Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub command: DaemonCommand,
}

#[derive(Subcommand)]
pub enum DaemonCommand {
    /// Start Ralph as a background daemon (continuous execution mode).
    #[command(
        about = "Start Ralph as a background daemon",
        after_long_help = "Examples:
 ralph daemon start
 ralph daemon start --empty-poll-ms 5000
 ralph daemon start --wait-poll-ms 500"
    )]
    Start(DaemonStartArgs),
    /// Request the daemon to stop gracefully.
    #[command(
        about = "Request the daemon to stop",
        after_long_help = "Examples:
 ralph daemon stop"
    )]
    Stop,
    /// Show daemon status (running, stopped, or stale).
    #[command(
        about = "Show daemon status",
        after_long_help = "Examples:
 ralph daemon status"
    )]
    Status,
    /// Internal: Run the daemon serve loop (do not use directly).
    #[command(hide = true)]
    Serve(DaemonServeArgs),
}

#[derive(Args)]
pub struct DaemonStartArgs {
    /// Poll interval in milliseconds while waiting for new tasks when queue is empty
    /// (default: 30000, min: 50).
    #[arg(long, default_value_t = 30_000)]
    pub empty_poll_ms: u64,

    /// Poll interval in milliseconds while waiting for blocked tasks (default: 1000, min: 50).
    #[arg(long, default_value_t = 1_000)]
    pub wait_poll_ms: u64,

    /// Notify when queue becomes unblocked (desktop + webhook).
    #[arg(long)]
    pub notify_when_unblocked: bool,
}

#[derive(Args)]
pub struct DaemonServeArgs {
    /// Poll interval in milliseconds while waiting for new tasks when queue is empty
    /// (default: 30000, min: 50).
    #[arg(long, default_value_t = 30_000)]
    pub empty_poll_ms: u64,

    /// Poll interval in milliseconds while waiting for blocked tasks (default: 1000, min: 50).
    #[arg(long, default_value_t = 1_000)]
    pub wait_poll_ms: u64,

    /// Notify when queue becomes unblocked (desktop + webhook).
    #[arg(long)]
    pub notify_when_unblocked: bool,
}
