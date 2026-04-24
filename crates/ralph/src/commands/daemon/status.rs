//! Daemon status command implementation.
//!
//! Purpose:
//! - Daemon status command implementation.
//!
//! Responsibilities:
//! - Display daemon runtime status to users (running/stopped/indeterminate).
//! - Report daemon PID, start time, and command when running.
//! - Detect and clean up stale state files when the daemon is not running.
//! - Provide manual cleanup instructions for indeterminate states.
//!
//! Not handled here:
//! - Starting or stopping the daemon (see `super::start` and `super::stop`).
//! - Daemon log inspection (see `super::logs`).
//! - PID liveness checks themselves (handled in `crate::lock`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - State file lives at `{cache_dir}/daemon.json`.
//! - Lock directory lives at `{cache_dir}/daemon.lock`.
//! - Uses `PidLiveness` to determine if the daemon process is actually running.

use crate::config::Resolved;
use crate::lock::PidLiveness;
use anyhow::Result;

use super::{
    clear_daemon_runtime_artifacts, daemon_pid_liveness, get_daemon_state,
    manual_daemon_cleanup_instructions,
};

/// Show daemon status.
pub fn status(resolved: &Resolved) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");

    match get_daemon_state(&cache_dir)? {
        Some(state) => match daemon_pid_liveness(state.pid) {
            PidLiveness::Running => {
                println!("Daemon is running");
                println!("  PID: {}", state.pid);
                println!("  Started: {}", state.started_at);
                println!("  Command: {}", state.command);
            }
            PidLiveness::NotRunning => {
                println!("Daemon is not running (stale state file detected)");
                println!("  Last PID: {}", state.pid);
                println!("  Last started: {}", state.started_at);
                clear_daemon_runtime_artifacts(&cache_dir, true);
            }
            PidLiveness::Indeterminate => {
                println!(
                    "Daemon PID liveness is indeterminate; preserving state/lock \
                         to avoid concurrent supervisors."
                );
                println!("  PID: {}", state.pid);
                println!("  Started: {}", state.started_at);
                println!("  Command: {}", state.command);
                println!();
                println!("{}", manual_daemon_cleanup_instructions(&cache_dir));
            }
        },
        None => {
            println!("Daemon is not running");
        }
    }

    Ok(())
}
