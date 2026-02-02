//! Queue repair subcommand.

use anyhow::Result;
use clap::Args;

use crate::config::Resolved;
use crate::queue;

/// Arguments for `ralph queue repair`.
#[derive(Args)]
pub struct RepairArgs {
    /// Show what would be changed without writing to disk.
    #[arg(long)]
    pub dry_run: bool,
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: RepairArgs) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue repair", force)?;
    let report = queue::repair_queue(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
        args.dry_run,
    )?;

    if report.is_empty() {
        log::info!("No issues found. Queue is healthy.");
    } else {
        log::info!("Repair report:");
        if report.fixed_tasks > 0 {
            log::info!("  Fixed missing fields in {} task(s)", report.fixed_tasks);
        }
        if report.fixed_timestamps > 0 {
            log::info!(
                "  Fixed invalid timestamps in {} task(s)",
                report.fixed_timestamps
            );
        }
        if !report.remapped_ids.is_empty() {
            log::info!("  Remapped {} duplicate ID(s):", report.remapped_ids.len());
            for (old, new) in &report.remapped_ids {
                log::info!("    {} -> {}", old, new);
            }
        }
        if args.dry_run {
            log::info!("Dry run: no changes written to disk.");
        } else {
            log::info!("Repaired queue written to disk.");
        }
    }

    Ok(())
}
