//! Queue stop subcommand.
//!
//! Responsibilities:
//! - Create a stop signal file to request graceful termination of a running loop.
//!
//! Not handled here:
//! - Signal detection in the run loop (see `crate::commands::run`).
//! - Signal cleanup (handled by the run loop after detection).
//!
//! Invariants/assumptions:
//! - The stop signal is a file-based flag, not a process signal.
//! - Multiple stop commands are idempotent (subsequent calls are no-ops).
//! - The signal is only acted upon between tasks, not during task execution.

use anyhow::Result;

use crate::config::Resolved;
use crate::signal;

pub(crate) fn handle(resolved: &Resolved) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");

    signal::create_stop_signal(&cache_dir)?;

    log::info!(
        "Stop signal created. The loop will stop starting new tasks and exit when current in-flight work completes."
    );
    log::info!(
        "Sequential mode: exits between tasks. Parallel mode: stops scheduling new tasks and waits for in-flight tasks."
    );
    log::info!("To force immediate termination, press Ctrl+C in the running loop.");

    Ok(())
}
