//! Purpose: Enforce undo snapshot retention limits.
//!
//! Responsibilities:
//! - Identify undo snapshot files eligible for pruning.
//! - Remove oldest snapshots once the configured limit is exceeded.
//!
//! Scope:
//! - Retention only; snapshot creation/listing/loading and restore live in
//!   sibling modules.
//!
//! Usage:
//! - Called after snapshot creation through `crate::undo::prune_old_undo_snapshots`.
//!
//! Invariants/Assumptions:
//! - Snapshot filenames sort chronologically because they embed normalized timestamps.
//! - Non-snapshot files in the undo cache directory are ignored.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::storage::UNDO_SNAPSHOT_PREFIX;

/// Prune old snapshots to enforce retention limit.
///
/// Returns the number of snapshots removed.
pub fn prune_old_undo_snapshots(undo_dir: &Path, max_count: usize) -> Result<usize> {
    if max_count == 0 || !undo_dir.exists() {
        return Ok(0);
    }

    let mut snapshot_paths: Vec<PathBuf> = Vec::new();

    for entry in std::fs::read_dir(undo_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.extension().map(|ext| ext == "json").unwrap_or(false) {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy();
        if filename.starts_with(UNDO_SNAPSHOT_PREFIX) {
            snapshot_paths.push(path);
        }
    }

    if snapshot_paths.len() <= max_count {
        return Ok(0);
    }

    snapshot_paths.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });

    let to_remove = snapshot_paths.len() - max_count;
    let mut removed = 0;

    for path in snapshot_paths.into_iter().take(to_remove) {
        match std::fs::remove_file(&path) {
            Ok(_) => {
                removed += 1;
                log::debug!("pruned old undo snapshot: {}", path.display());
            }
            Err(err) => {
                log::warn!(
                    "failed to remove old snapshot {}: {:#}",
                    path.display(),
                    err
                )
            }
        }
    }

    Ok(removed)
}
