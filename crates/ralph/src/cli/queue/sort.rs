//! Queue sort subcommand.
//!
//! Responsibilities:
//! - Reorder tasks in .ralph/queue.jsonc by priority.
//! - Support dry-run mode to preview the new order.
//!
//! Not handled here:
//! - Time-based or complex sorting (use `ralph queue list --sort-by` for that).
//! - Sorting by arbitrary fields (intentionally limited to prevent footguns).
//!
//! Invariants/assumptions:
//! - Only supports priority sorting to avoid dangerous queue reordering.
//! - Mutates queue.json; use with care in collaborative environments.
//! - Dry-run mode does NOT create undo snapshots or write to disk.

use anyhow::Result;
use clap::Args;

use crate::config::Resolved;
use crate::queue;

use super::{QueueSortBy, QueueSortOrder};

/// Arguments for `ralph queue sort`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue sort\n  ralph queue sort --order descending\n  ralph queue sort --order ascending\n  ralph queue sort --dry-run\n  ralph queue list --scheduled --sort-by scheduled_start --order ascending\n\nDry run:\n  Shows the new queue order without modifying files.\n\nNote:\n  - `ralph queue sort` reorders .ralph/queue.jsonc and intentionally supports priority only.\n  - For triage/time-based sorting without mutating files, use `ralph queue list --sort-by ...`."
)]
pub struct QueueSortArgs {
    /// Sort by field (supported: priority only; reorders queue file).
    #[arg(long, value_enum, default_value_t = QueueSortBy::Priority)]
    pub sort_by: QueueSortBy,

    /// Sort order (default: descending, highest priority first).
    #[arg(long, value_enum, default_value_t = QueueSortOrder::Descending)]
    pub order: QueueSortOrder,

    /// Show what would change without writing to disk.
    #[arg(long)]
    pub dry_run: bool,
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueueSortArgs) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue sort", force)?;

    // Create undo snapshot before mutation (only if not dry-run)
    if !args.dry_run {
        crate::undo::create_undo_snapshot(resolved, &format!("queue sort by {}", args.sort_by))?;
    }

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    // Capture original order for dry-run comparison
    let original_ids: Vec<String> = queue_file.tasks.iter().map(|t| t.id.clone()).collect();

    match args.sort_by {
        QueueSortBy::Priority => {
            queue::sort_tasks_by_priority(&mut queue_file, args.order.is_descending());
        }
    }

    if args.dry_run {
        let new_ids: Vec<String> = queue_file.tasks.iter().map(|t| t.id.clone()).collect();
        if original_ids == new_ids {
            log::info!("Dry run: queue is already sorted (no changes).");
        } else {
            log::info!("Dry run: would reorder {} task(s).", new_ids.len());
            log::info!("New order: {}", new_ids.join(", "));
        }
    } else {
        queue::save_queue(&resolved.queue_path, &queue_file)?;
        log::info!("Queue sorted by {} (order: {}).", args.sort_by, args.order);
    }
    Ok(())
}
