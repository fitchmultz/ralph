//! Daemon stop command implementation.
//!
//! Purpose:
//! - Daemon stop command implementation.
//!
//! Responsibilities:
//! - Stop a running Ralph daemon process gracefully.
//! - Clean up daemon state and lock files after stopping.
//! - Handle cases where daemon is not running or state is stale.
//!
//! Non-scope:
//! - Starting or restarting the daemon (handled by start command).
//! - Windows service management (Unix-only implementation).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Daemon uses a dedicated lock at `.ralph/cache/daemon.lock`.
//! - Daemon state is stored at `.ralph/cache/daemon.json`.
//! - Stop signal is created via `crate::signal::create_stop_signal`.

use crate::config::Resolved;
use crate::lock::PidLiveness;
use anyhow::{Context, Result, bail};
use std::time::Duration;

use super::{
    clear_daemon_runtime_artifacts, daemon_pid_liveness, get_daemon_state,
    manual_daemon_cleanup_instructions, wait_for_daemon_shutdown,
};

/// Stop the daemon gracefully.
pub fn stop(resolved: &Resolved) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");

    // Check if daemon is running
    let state = match get_daemon_state(&cache_dir)? {
        Some(state) => state,
        None => {
            println!("Daemon is not running");
            return Ok(());
        }
    };

    match daemon_pid_liveness(state.pid) {
        PidLiveness::NotRunning => {
            println!("Daemon is not running (removing stale daemon artifacts)");
            clear_daemon_runtime_artifacts(&cache_dir, true);
            return Ok(());
        }
        PidLiveness::Indeterminate => {
            bail!(
                "Daemon PID {} liveness is indeterminate; preserving state/lock to avoid concurrent supervisors. \
                 {}",
                state.pid,
                manual_daemon_cleanup_instructions(&cache_dir)
            );
        }
        PidLiveness::Running => {}
    }

    // Create stop signal
    crate::signal::create_stop_signal(&cache_dir).context("Failed to create stop signal")?;
    println!("Stop signal sent to daemon (PID: {})", state.pid);

    // Wait up to 10 seconds for the daemon to exit
    println!("Waiting for daemon to stop...");
    if wait_for_daemon_shutdown(&cache_dir, state.pid, Duration::from_secs(10))? {
        println!("Daemon stopped successfully");
        clear_daemon_runtime_artifacts(&cache_dir, true);
        return Ok(());
    }

    // Daemon didn't stop in time
    bail!(
        "Daemon did not stop within 10 seconds. PID: {}. You may need to kill it manually with `kill -9 {}`",
        state.pid,
        state.pid
    );
}
