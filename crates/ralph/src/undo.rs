//! Purpose: Provide the public undo API for queue snapshot creation, listing,
//! restore, and retention.
//!
//! Responsibilities:
//! - Declare the `undo` child modules.
//! - Re-export the stable undo data models and operations.
//!
//! Scope:
//! - Thin facade only; implementation lives in sibling files under `undo/`.
//!
//! Usage:
//! - Import undo helpers through `crate::undo`.
//!
//! Invariants/Assumptions:
//! - The public undo API remains stable across this split.
//! - Snapshot creation, restore, and pruning behavior remain unchanged.

mod model;
mod prune;
mod restore;
mod storage;

#[cfg(test)]
mod tests;

pub use model::{RestoreResult, SnapshotList, UndoSnapshot, UndoSnapshotMeta};
pub use prune::prune_old_undo_snapshots;
pub use restore::restore_from_snapshot;
pub use storage::{create_undo_snapshot, list_undo_snapshots, load_undo_snapshot, undo_cache_dir};

#[cfg(test)]
pub(crate) use storage::UNDO_SNAPSHOT_PREFIX;
