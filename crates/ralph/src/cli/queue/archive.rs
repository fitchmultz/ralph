//! Queue archive subcommand.
//!
//! Responsibilities:
//! - Move terminal tasks (done/rejected) from queue.json to done.json.
//! - Support dry-run mode to preview what would be archived.
//!
//! Not handled here:
//! - Automatic archiving based on task age (see `queue.auto_archive_terminal_after_days`).
//! - Archive internals (see `crate::queue::operations::archive`).
//!
//! Invariants/assumptions:
//! - Dry-run mode does NOT create undo snapshots or write to disk.
//! - Normal mode creates an undo snapshot before mutation.

use anyhow::Result;
use clap::Args;

use crate::config::Resolved;
use crate::queue;

/// Arguments for `ralph queue archive`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue archive\n  ralph queue archive --dry-run\n\nDry run:\n  Shows what would be archived without modifying files."
)]
pub struct QueueArchiveArgs {
    /// Show what would be archived without writing to disk.
    #[arg(long)]
    pub dry_run: bool,
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueueArchiveArgs) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue archive", force)?;

    // Create undo snapshot before mutation (only if not dry-run)
    if !args.dry_run {
        crate::undo::create_undo_snapshot(resolved, "queue archive")?;
    }

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    if args.dry_run {
        // Dry-run: compute what would be archived without modifying files
        let mut active = queue::load_queue(&resolved.queue_path)?;
        let mut done = queue::load_queue_or_default(&resolved.done_path)?;
        let now = crate::timeutil::now_utc_rfc3339()?;
        let report = queue::archive_terminal_tasks_in_memory(&mut active, &mut done, &now)?;

        if report.moved_ids.is_empty() {
            log::info!("Dry run: no terminal tasks to archive.");
        } else {
            log::info!(
                "Dry run: would archive {} terminal task(s).",
                report.moved_ids.len()
            );
            log::info!("Task IDs: {}", report.moved_ids.join(", "));
        }
    } else {
        // Normal execution
        let report = queue::archive_terminal_tasks(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )?;
        if report.moved_ids.is_empty() {
            log::info!("No terminal tasks (done/rejected) to archive.");
        } else {
            log::info!(
                "Archived {} terminal task(s) (done/rejected).",
                report.moved_ids.len()
            );
        }
    }
    Ok(())
}
