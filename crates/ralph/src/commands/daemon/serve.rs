//! Internal daemon serve command implementation.
//!
//! Responsibilities:
//! - Run the continuous execution loop in daemon mode.
//! - Acquire and hold the daemon lock for process lifecycle.
//! - Write and clean up daemon state on startup/shutdown.
//!
//! Not handled here:
//! - CLI argument parsing (handled in `crate::cli::daemon`).
//! - Daemon start/stop/status control (handled in parent `mod.rs`).
//! - Signal handling (handled by `crate::signal`).
//!
//! Invariants/assumptions:
//! - This function is internal and should not be called directly by users.
//! - The daemon lock must be acquired before writing state.
//! - State cleanup occurs regardless of the run_loop result.
//! - Log output is redirected to `.ralph/logs/daemon.log` by the parent.

use crate::cli::daemon::DaemonServeArgs;
use crate::config::Resolved;
use anyhow::{Context, Result};
use std::fs;

use super::{DAEMON_LOCK_DIR, DAEMON_STATE_FILE, DaemonState, write_daemon_state};

/// Internal: Run the daemon serve loop.
/// This should not be called directly by users.
pub fn serve(resolved: &Resolved, args: DaemonServeArgs) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let daemon_lock_dir = cache_dir.join(DAEMON_LOCK_DIR);

    // Acquire the daemon lock
    let _lock = crate::lock::acquire_dir_lock(&daemon_lock_dir, "daemon", false)
        .context("Failed to acquire daemon lock")?;

    // Write daemon state
    let state = DaemonState {
        version: 1,
        pid: std::process::id(),
        started_at: crate::timeutil::now_utc_rfc3339()?,
        repo_root: resolved.repo_root.display().to_string(),
        command: std::env::args().collect::<Vec<_>>().join(" "),
    };
    write_daemon_state(&cache_dir, &state)?;

    log::info!(
        "Daemon started (PID: {}, empty_poll={}ms, wait_poll={}ms)",
        state.pid,
        args.empty_poll_ms,
        args.wait_poll_ms
    );

    // Run the continuous execution loop
    let result = crate::commands::run::run_loop(
        resolved,
        crate::commands::run::RunLoopOptions {
            max_tasks: 0, // No limit in daemon mode
            agent_overrides: crate::agent::AgentOverrides::default(),
            force: true, // Force mode for unattended operation
            auto_resume: false,
            starting_completed: 0,
            non_interactive: true,
            parallel_workers: None,
            wait_when_blocked: true,
            wait_poll_ms: args.wait_poll_ms,
            wait_timeout_seconds: 0, // No timeout in daemon mode
            notify_when_unblocked: args.notify_when_unblocked,
            wait_when_empty: true,
            empty_poll_ms: args.empty_poll_ms,
        },
    );

    // Clean up state on exit
    log::info!("Daemon shutting down");
    let state_path = cache_dir.join(DAEMON_STATE_FILE);
    if let Err(e) = fs::remove_file(&state_path) {
        log::debug!(
            "Failed to remove daemon state file on shutdown {}: {}",
            state_path.display(),
            e
        );
    }

    result
}
