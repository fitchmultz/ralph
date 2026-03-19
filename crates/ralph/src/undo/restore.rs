//! Purpose: Restore queue state from undo snapshots.
//!
//! Responsibilities:
//! - Resolve the target snapshot to restore.
//! - Overwrite queue and done files from stored snapshot state.
//! - Remove consumed snapshots after successful restore.
//!
//! Scope:
//! - Restore orchestration only; snapshot storage and pruning live in sibling
//!   modules.
//!
//! Usage:
//! - Called through `crate::undo::restore_from_snapshot` by the undo CLI.
//!
//! Invariants/Assumptions:
//! - Callers hold the queue lock before invoking restore.
//! - Successful restores remove the used snapshot to avoid redo cycles.

use anyhow::{Result, bail};

use crate::config::Resolved;
use crate::queue::save_queue;

use super::model::RestoreResult;
use super::storage::{
    UNDO_SNAPSHOT_PREFIX, list_undo_snapshots, load_undo_snapshot, undo_cache_dir,
};

/// Restore queue state from a snapshot.
///
/// This overwrites both queue.json and done.json with the snapshot content.
/// Caller must hold the queue lock.
pub fn restore_from_snapshot(
    resolved: &Resolved,
    snapshot_id: Option<&str>,
    dry_run: bool,
) -> Result<RestoreResult> {
    let list = list_undo_snapshots(&resolved.repo_root)?;

    if list.snapshots.is_empty() {
        bail!("No undo snapshots available");
    }

    let target_id = snapshot_id
        .map(str::to_string)
        .unwrap_or_else(|| list.snapshots[0].id.clone());
    let snapshot = load_undo_snapshot(&resolved.repo_root, &target_id)?;
    let tasks_affected = snapshot.queue_json.tasks.len() + snapshot.done_json.tasks.len();

    if dry_run {
        return Ok(RestoreResult {
            operation: snapshot.operation,
            timestamp: snapshot.timestamp,
            tasks_affected,
        });
    }

    save_queue(&resolved.done_path, &snapshot.done_json)?;
    save_queue(&resolved.queue_path, &snapshot.queue_json)?;

    let undo_dir = undo_cache_dir(&resolved.repo_root);
    let snapshot_path = undo_dir.join(format!("{}{}.json", UNDO_SNAPSHOT_PREFIX, target_id));
    if let Err(err) = std::fs::remove_file(&snapshot_path) {
        log::warn!("failed to remove used snapshot: {:#}", err);
    }

    log::info!(
        "restored queue state from snapshot '{}' (operation: {}, {} tasks affected)",
        target_id,
        snapshot.operation,
        tasks_affected
    );

    Ok(RestoreResult {
        operation: snapshot.operation,
        timestamp: snapshot.timestamp,
        tasks_affected,
    })
}
