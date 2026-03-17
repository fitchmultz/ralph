//! Purpose: Shared data types for done-queue pruning operations.
//!
//! Responsibilities:
//! - Define prune input options.
//! - Define prune result reporting.
//!
//! Scope:
//! - Type definitions only; no queue IO or pruning logic lives here.
//!
//! Usage:
//! - Consumed by prune core logic, queue re-exports, and CLI callers.
//!
//! Invariants/Assumptions:
//! - `PruneOptions` remains the stable input contract for pruning.
//! - `PruneReport` remains the stable result contract for dry-run and live pruning.

use crate::contracts::TaskStatus;
use std::collections::HashSet;

/// Result of a prune operation on the done archive.
#[derive(Debug, Clone, Default)]
pub struct PruneReport {
    /// IDs of tasks that were pruned (or would be pruned in dry-run).
    pub pruned_ids: Vec<String>,
    /// IDs of tasks that were kept (protected by keep-last or didn't match filters).
    pub kept_ids: Vec<String>,
}

/// Options for pruning tasks from the done archive.
#[derive(Debug, Clone)]
pub struct PruneOptions {
    /// Minimum age in days for a task to be pruned (None = no age filter).
    pub age_days: Option<u32>,
    /// Statuses to prune (empty = all statuses).
    pub statuses: HashSet<TaskStatus>,
    /// Keep the N most recently completed tasks regardless of other filters.
    pub keep_last: Option<u32>,
    /// If true, report what would be pruned without writing to disk.
    pub dry_run: bool,
}
