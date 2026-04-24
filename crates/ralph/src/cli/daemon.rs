//! `ralph daemon ...` command group for background service management.
//!
//! Purpose:
//! - `ralph daemon ...` command group for background service management.
//!
//! Responsibilities:
//! - Define clap structures for daemon commands and flags.
//! - Route daemon subcommands to the daemon command implementations.
//!
//! Not handled here:
//! - Daemon process management (see `crate::commands::daemon`).
//! - Queue execution logic (see `crate::commands::run`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Daemon is Unix-only (Windows uses different service mechanisms).
//! - Daemon uses a dedicated lock separate from the queue lock.

use anyhow::Result;
use clap::{Args, Subcommand, builder::PossibleValuesParser};

use crate::{commands::daemon as daemon_cmd, config};

pub fn handle_daemon(cmd: DaemonCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        DaemonCommand::Start(args) => daemon_cmd::start(&resolved, args),
        DaemonCommand::Stop => daemon_cmd::stop(&resolved),
        DaemonCommand::Status => daemon_cmd::status(&resolved),
        DaemonCommand::Serve(args) => daemon_cmd::serve(&resolved, args),
        DaemonCommand::Logs(args) => daemon_cmd::logs(&resolved, args),
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
    /// Inspect daemon logs with filtering and follow mode.
    #[command(
        about = "Inspect daemon logs",
        after_long_help = "Examples:
 ralph daemon logs
 ralph daemon logs --tail 50
 ralph daemon logs --follow --tail 200
 ralph daemon logs --since 'in 10 minutes'
 ralph daemon logs --level error --contains \"webhook\"
 ralph daemon logs --json --since 2026-02-01T00:00:00Z"
    )]
    Logs(DaemonLogsArgs),
}

#[derive(Args)]
pub struct DaemonStartArgs {
    /// Poll interval in milliseconds while waiting for new tasks when queue is empty
    /// (default: 30000, min: 50).
    #[arg(
        long,
        default_value_t = 30_000,
        value_parser = clap::value_parser!(u64).range(50..)
    )]
    pub empty_poll_ms: u64,

    /// Poll interval in milliseconds while waiting for blocked tasks (default: 1000, min: 50).
    #[arg(
        long,
        default_value_t = 1_000,
        value_parser = clap::value_parser!(u64).range(50..)
    )]
    pub wait_poll_ms: u64,

    /// Notify when queue becomes unblocked (desktop + webhook).
    #[arg(long)]
    pub notify_when_unblocked: bool,
}

#[derive(Args)]
pub struct DaemonServeArgs {
    /// Poll interval in milliseconds while waiting for new tasks when queue is empty
    /// (default: 30000, min: 50).
    #[arg(
        long,
        default_value_t = 30_000,
        value_parser = clap::value_parser!(u64).range(50..)
    )]
    pub empty_poll_ms: u64,

    /// Poll interval in milliseconds while waiting for blocked tasks (default: 1000, min: 50).
    #[arg(
        long,
        default_value_t = 1_000,
        value_parser = clap::value_parser!(u64).range(50..)
    )]
    pub wait_poll_ms: u64,

    /// Notify when queue becomes unblocked (desktop + webhook).
    #[arg(long)]
    pub notify_when_unblocked: bool,
}

#[derive(Args)]
pub struct DaemonLogsArgs {
    /// Show the last N lines from the daemon log.
    #[arg(short = 'n', long = "tail", default_value_t = 100)]
    pub tail: usize,

    /// Follow daemon log output as lines are appended.
    #[arg(short, long)]
    pub follow: bool,

    /// Only show lines at or after this timestamp (RFC3339) or relative expression.
    #[arg(long, value_name = "DURATION_OR_TIMESTAMP", value_parser = parse_daemon_log_since)]
    pub since: Option<time::OffsetDateTime>,

    /// Filter by level (trace, debug, info, warn, error, fatal, critical).
    #[arg(long = "level", value_name = "LEVEL", value_parser = PossibleValuesParser::new([
        "trace",
        "debug",
        "info",
        "warn",
        "error",
        "fatal",
        "critical"
    ]))]
    pub level: Option<String>,

    /// Show only lines containing this substring.
    #[arg(long)]
    pub contains: Option<String>,

    /// Emit machine-readable JSON objects, one per output line.
    #[arg(long)]
    pub json: bool,
}

fn parse_daemon_log_since(raw: &str) -> anyhow::Result<time::OffsetDateTime> {
    crate::timeutil::parse_relative_time(raw)
        .and_then(|value| crate::timeutil::parse_rfc3339(&value))
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::Cli;

    #[test]
    fn daemon_start_wait_poll_ms_rejects_below_minimum() {
        let args = vec!["ralph", "daemon", "start", "--wait-poll-ms", "10"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "daemon start --wait-poll-ms should reject values below 50"
        );
    }

    #[test]
    fn daemon_start_empty_poll_ms_rejects_below_minimum() {
        let args = vec!["ralph", "daemon", "start", "--empty-poll-ms", "10"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "daemon start --empty-poll-ms should reject values below 50"
        );
    }

    #[test]
    fn daemon_start_wait_poll_ms_accepts_minimum() {
        let args = vec!["ralph", "daemon", "start", "--wait-poll-ms", "50"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_ok(),
            "daemon start --wait-poll-ms should accept 50"
        );
    }

    #[test]
    fn daemon_start_empty_poll_ms_accepts_minimum() {
        let args = vec!["ralph", "daemon", "start", "--empty-poll-ms", "50"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_ok(),
            "daemon start --empty-poll-ms should accept 50"
        );
    }
}
