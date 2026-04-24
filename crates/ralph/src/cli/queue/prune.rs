//! Queue prune subcommand.
//!
//! Purpose:
//! - Queue prune subcommand.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Result;
use clap::Args;

use crate::config::Resolved;
use crate::queue;

use super::StatusArg;

/// Arguments for `ralph queue prune`.
#[derive(Args)]
#[command(
    after_long_help = "Prune removes old tasks from .ralph/done.jsonc while preserving recent history.\n\nSafety:\n  --keep-last always protects the N most recently completed tasks (by completed_at).\n  If no filters are provided, all tasks are pruned except those protected by --keep-last.\n  Missing or invalid completed_at timestamps are treated as oldest for keep-last ordering\n  but do NOT match the age filter (safety-first).\n\nExamples:\n  ralph queue prune --dry-run --age 30 --status rejected\n  ralph queue prune --keep-last 100\n  ralph queue prune --age 90\n  ralph queue prune --age 30 --status done --keep-last 50"
)]
pub struct QueuePruneArgs {
    /// Only prune tasks completed at least N days ago.
    #[arg(long)]
    pub age: Option<u32>,

    /// Filter by task status (repeatable).
    #[arg(long, value_enum)]
    pub status: Vec<StatusArg>,

    /// Keep the N most recently completed tasks regardless of filters.
    #[arg(long)]
    pub keep_last: Option<u32>,

    /// Show what would be pruned without writing to disk.
    #[arg(long)]
    pub dry_run: bool,
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueuePruneArgs) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue prune", force)?;

    // Create undo snapshot before mutation (only if not dry-run)
    if !args.dry_run {
        crate::undo::create_undo_snapshot(resolved, "queue prune")?;
    }

    let report: queue::PruneReport = queue::prune_done_tasks(
        &resolved.done_path,
        queue::PruneOptions {
            age_days: args.age,
            statuses: args.status.into_iter().map(|s| s.into()).collect(),
            keep_last: args.keep_last,
            dry_run: args.dry_run,
        },
    )?;
    if args.dry_run {
        log::info!("Dry run: would prune {} task(s).", report.pruned_ids.len());
        if !report.pruned_ids.is_empty() {
            log::info!("Pruned IDs: {}", report.pruned_ids.join(", "));
        }
        if !report.kept_ids.is_empty() {
            log::info!("Kept IDs: {}", report.kept_ids.join(", "));
        }
    } else {
        if report.pruned_ids.is_empty() {
            log::info!("No tasks pruned.");
        } else {
            log::info!("Pruned {} task(s).", report.pruned_ids.len());
        }
        if !report.kept_ids.is_empty() {
            log::debug!("Kept {} task(s).", report.kept_ids.len());
        }
    }
    Ok(())
}
