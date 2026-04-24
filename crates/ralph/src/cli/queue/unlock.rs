//! Queue unlock subcommand.
//!
//! Purpose:
//! - Queue unlock subcommand.
//!
//! Responsibilities:
//! - Remove the queue lock directory with safety checks.
//! - Detect active lock holders and require explicit override.
//! - Provide dry-run preview of lock state.
//!
//! Not handled here:
//! - Lock acquisition (see `crate::lock`).
//! - Queue file validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Confirmation is required in interactive contexts unless --yes is set.
//! - Active process detection is conservative: indeterminate status is treated as running.

use anyhow::{Context, Result, bail};
use clap::Args;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::config::Resolved;
use crate::lock::{self, PidLiveness};

/// Arguments for `ralph queue unlock`.
#[derive(Args)]
#[command(after_long_help = "Safely remove the queue lock directory.\n\n\
Safety:\n  - Checks if the lock holder process is still running\n  - Blocks if process is active (override with --force)\n  - Requires confirmation in interactive mode (bypass with --yes)\n\n\
Examples:\n  ralph queue unlock --dry-run\n  ralph queue unlock --yes\n  ralph queue unlock --force --yes\n  ralph queue unlock --force  # Still requires confirmation")]
pub struct QueueUnlockArgs {
    /// Override active process check and remove lock anyway.
    #[arg(long)]
    pub force: bool,

    /// Skip confirmation prompt (use with caution).
    #[arg(long)]
    pub yes: bool,

    /// Show what would happen without removing the lock.
    #[arg(long)]
    pub dry_run: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueUnlockArgs) -> Result<()> {
    let lock_dir = lock::queue_lock_dir(&resolved.repo_root);

    if !lock_dir.exists() {
        log::info!("Queue is not locked.");
        return Ok(());
    }

    // Read lock owner metadata
    let owner = lock::read_lock_owner(&lock_dir)?;
    let owner_info = format_owner_info(&owner);

    // Check if lock holder process is still active
    let staleness = owner.as_ref().map(lock::classify_lock_owner);
    let is_active = staleness
        .as_ref()
        .is_some_and(|staleness| !staleness.is_stale());

    // Determine action based on state and flags
    if args.dry_run {
        handle_dry_run(&lock_dir, &owner_info, is_active, staleness)?;
        return Ok(());
    }

    // Safety check: block if process is active unless --force is set
    if is_active && !args.force {
        let pid_str = owner
            .as_ref()
            .map(|o| o.pid.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        bail!(
            "Refusing to unlock: lock holder process (PID {}) appears to be still running.\n\
             Lock holder: {}\n\n\
             Use --force to override this check, or verify the process has exited.\n\
             Example: ralph queue unlock --force --yes",
            pid_str,
            owner_info
        );
    }

    // Require confirmation in interactive contexts
    if !args.yes
        && is_terminal_context()
        && !confirm_unlock(&lock_dir, &owner_info, is_active, args.force)?
    {
        log::info!("Unlock cancelled.");
        return Ok(());
    }

    // Warn when force-removing an active lock
    if is_active && args.force {
        log::warn!(
            "Force-removing lock for active process (PID: {}). Queue corruption may occur if the process is still writing.",
            owner.as_ref().map(|o| o.pid).unwrap_or(0)
        );
    }

    // Perform the unlock
    std::fs::remove_dir_all(&lock_dir)
        .with_context(|| format!("remove lock dir {}", lock_dir.display()))?;

    log::info!("Queue unlocked (removed {}).", lock_dir.display());
    Ok(())
}

fn format_owner_info(owner: &Option<lock::LockOwner>) -> String {
    match owner {
        Some(o) => format!(
            "PID={}, label={}, started={}, command={}",
            o.pid, o.label, o.started_at, o.command
        ),
        None => "(owner metadata missing)".to_string(),
    }
}

fn handle_dry_run(
    lock_dir: &Path,
    owner_info: &str,
    is_active: bool,
    staleness: Option<lock::LockStaleness>,
) -> Result<()> {
    println!("Lock directory: {}", lock_dir.display());
    println!("Lock holder: {}", owner_info);

    match staleness.map(|staleness| staleness.liveness) {
        Some(PidLiveness::Running) => {
            println!("Process status: RUNNING (unlock would be blocked without --force)");
        }
        Some(PidLiveness::NotRunning) => {
            println!("Process status: NOT RUNNING (safe to unlock)");
        }
        Some(PidLiveness::Indeterminate) => {
            println!("Process status: INDETERMINATE (unlock would be blocked without --force)");
        }
        None => {
            println!("Process status: UNKNOWN (no owner metadata)");
        }
    }

    if let Some(note) = staleness.and_then(lock::LockStaleness::advisory_note) {
        println!("Staleness policy: {}", note.trim());
    }

    if is_active {
        println!(
            "Would remove: {} (blocked without --force)",
            lock_dir.display()
        );
    } else {
        println!("Would remove: {} (safe to unlock)", lock_dir.display());
    }

    println!("Dry run: no changes made.");
    Ok(())
}

fn is_terminal_context() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn confirm_unlock(lock_dir: &Path, owner_info: &str, is_active: bool, force: bool) -> Result<bool> {
    println!("About to remove lock directory: {}", lock_dir.display());
    println!("Lock holder: {}", owner_info);
    if is_active {
        if force {
            println!("WARNING: Force-removing lock for ACTIVE process!");
            println!("         Data corruption may occur if the process is still writing.");
        } else {
            println!("WARNING: Lock holder process may still be active!");
        }
    }
    print!("Proceed with unlock? [y/N]: ");
    io::stdout().flush().context("flush confirmation prompt")?;

    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .context("read confirmation input")?;

    Ok(matches!(
        response.trim().to_lowercase().as_str(),
        "y" | "yes"
    ))
}
