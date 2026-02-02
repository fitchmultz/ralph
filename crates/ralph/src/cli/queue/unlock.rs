//! Queue unlock subcommand.
//!
//! Responsibilities:
//! - Remove the queue lock directory when explicitly requested.
//!
//! Not handled here:
//! - Lock acquisition rules or stale PID checks (see `crate::lock`).
//! - Queue file validation.
//!
//! Invariants/assumptions:
//! - Caller intentionally bypasses lock safety by invoking this command.

use anyhow::{Context, Result};

use crate::config::Resolved;
use crate::lock;

pub(crate) fn handle(resolved: &Resolved) -> Result<()> {
    let lock_dir = lock::queue_lock_dir(&resolved.repo_root);
    if lock_dir.exists() {
        std::fs::remove_dir_all(&lock_dir)
            .with_context(|| format!("remove lock dir {}", lock_dir.display()))?;
        log::info!("Queue unlocked (removed {}).", lock_dir.display());
    } else {
        log::info!("Queue is not locked.");
    }
    Ok(())
}
