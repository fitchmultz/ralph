//! Purpose: Create, list, and load undo snapshot files.
//!
//! Responsibilities:
//! - Resolve the undo cache directory.
//! - Persist queue/done snapshots atomically.
//! - Enumerate and load stored undo snapshots.
//!
//! Scope:
//! - Snapshot storage only; restore execution and retention policy live in
//!   sibling modules.
//!
//! Usage:
//! - Called by queue-mutation paths before writes and by restore flows when
//!   locating snapshots.
//!
//! Invariants/Assumptions:
//! - Snapshots capture both queue and done files together.
//! - Snapshot files use the `undo-<timestamp>.json` naming contract.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::config::Resolved;
use crate::constants::limits::MAX_UNDO_SNAPSHOTS;
use crate::fsutil;
use crate::queue::load_queue_or_default;

use super::model::{SnapshotList, UndoSnapshot, UndoSnapshotMeta};
use super::prune::prune_old_undo_snapshots;

/// Snapshot filename prefix.
pub(crate) const UNDO_SNAPSHOT_PREFIX: &str = "undo-";

/// Get the undo cache directory path.
pub fn undo_cache_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph").join("cache").join("undo")
}

/// Create a snapshot before a mutation operation.
///
/// This should be called AFTER acquiring the queue lock but BEFORE
/// performing any modifications. The snapshot captures both queue.json
/// and done.json atomically.
pub fn create_undo_snapshot(resolved: &Resolved, operation: &str) -> Result<PathBuf> {
    let undo_dir = undo_cache_dir(&resolved.repo_root);
    std::fs::create_dir_all(&undo_dir)
        .with_context(|| format!("create undo directory {}", undo_dir.display()))?;

    let timestamp = crate::timeutil::now_utc_rfc3339()
        .context("failed to generate timestamp for undo snapshot")?;
    let snapshot_id = timestamp.replace([':', '.', '-'], "");
    let snapshot_filename = format!("{}{}.json", UNDO_SNAPSHOT_PREFIX, snapshot_id);
    let snapshot_path = undo_dir.join(snapshot_filename);

    let queue_json = load_queue_or_default(&resolved.queue_path)?;
    let done_json = load_queue_or_default(&resolved.done_path)?;

    let snapshot = UndoSnapshot {
        version: 1,
        operation: operation.to_string(),
        timestamp: timestamp.clone(),
        queue_json,
        done_json,
    };

    let content = serde_json::to_string_pretty(&snapshot)?;
    fsutil::write_atomic(&snapshot_path, content.as_bytes())
        .with_context(|| format!("write undo snapshot to {}", snapshot_path.display()))?;

    match prune_old_undo_snapshots(&undo_dir, MAX_UNDO_SNAPSHOTS) {
        Ok(pruned) if pruned > 0 => {
            log::debug!("pruned {} old undo snapshot(s)", pruned);
        }
        Ok(_) => {}
        Err(err) => {
            log::warn!("failed to prune undo snapshots: {:#}", err);
        }
    }

    log::debug!(
        "created undo snapshot for '{}' at {}",
        operation,
        snapshot_path.display()
    );

    Ok(snapshot_path)
}

/// List available undo snapshots, newest first.
pub fn list_undo_snapshots(repo_root: &Path) -> Result<SnapshotList> {
    let undo_dir = undo_cache_dir(repo_root);

    if !undo_dir.exists() {
        return Ok(SnapshotList {
            snapshots: Vec::new(),
        });
    }

    let mut snapshots = Vec::new();

    for entry in std::fs::read_dir(&undo_dir)
        .with_context(|| format!("read undo directory {}", undo_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if !path.extension().map(|ext| ext == "json").unwrap_or(false) {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy();
        if !filename.starts_with(UNDO_SNAPSHOT_PREFIX) {
            continue;
        }

        match extract_snapshot_meta(&path) {
            Ok(meta) => snapshots.push(meta),
            Err(err) => {
                log::warn!("failed to read snapshot {}: {:#}", path.display(), err);
            }
        }
    }

    snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(SnapshotList { snapshots })
}

/// Load a full snapshot by ID.
pub fn load_undo_snapshot(repo_root: &Path, snapshot_id: &str) -> Result<UndoSnapshot> {
    let undo_dir = undo_cache_dir(repo_root);
    let snapshot_filename = format!("{}{}.json", UNDO_SNAPSHOT_PREFIX, snapshot_id);
    let snapshot_path = undo_dir.join(snapshot_filename);

    if !snapshot_path.exists() {
        bail!("Snapshot not found: {}", snapshot_id);
    }

    let content = std::fs::read_to_string(&snapshot_path)?;
    let snapshot: UndoSnapshot = serde_json::from_str(&content)?;
    Ok(snapshot)
}

fn extract_snapshot_meta(path: &Path) -> Result<UndoSnapshotMeta> {
    let content = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| anyhow!("invalid snapshot filename: {}", path.display()))?
        .strip_prefix(UNDO_SNAPSHOT_PREFIX)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("invalid snapshot filename prefix: {}", path.display()))?;

    let operation = value
        .get("operation")
        .and_then(|raw| raw.as_str())
        .unwrap_or("unknown")
        .to_string();
    let timestamp = value
        .get("timestamp")
        .and_then(|raw| raw.as_str())
        .unwrap_or("")
        .to_string();

    Ok(UndoSnapshotMeta {
        id,
        operation,
        timestamp,
    })
}
