//! Purpose: Define undo snapshot data models and restore result types.
//!
//! Responsibilities:
//! - Define snapshot metadata and full snapshot payload types.
//! - Define list/restore result wrappers used by CLI and core flows.
//!
//! Scope:
//! - Data modeling only; file IO, restore, and pruning live in sibling modules.
//!
//! Usage:
//! - Used through `crate::undo` by CLI and queue-mutation paths.
//!
//! Invariants/Assumptions:
//! - Snapshot payloads serialize both queue and done files together.
//! - Snapshot IDs are timestamp-derived strings produced by storage helpers.

use crate::contracts::QueueFile;
use serde::{Deserialize, Serialize};

/// Metadata about a single undo snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoSnapshotMeta {
    /// Unique snapshot ID (timestamp-based).
    pub id: String,
    /// Human-readable operation description.
    pub operation: String,
    /// RFC3339 timestamp when snapshot was created.
    pub timestamp: String,
}

/// Full snapshot content (stored in JSON file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoSnapshot {
    /// Schema version for future migrations.
    pub version: u32,
    /// Human-readable operation description.
    pub operation: String,
    /// RFC3339 timestamp when snapshot was created.
    pub timestamp: String,
    /// Full queue.json content at snapshot time.
    pub queue_json: QueueFile,
    /// Full done.json content at snapshot time.
    pub done_json: QueueFile,
}

/// Result of listing snapshots.
#[derive(Debug, Clone)]
pub struct SnapshotList {
    pub snapshots: Vec<UndoSnapshotMeta>,
}

/// Result of a restore operation.
#[derive(Debug, Clone)]
pub struct RestoreResult {
    pub operation: String,
    pub timestamp: String,
    pub tasks_affected: usize,
}
