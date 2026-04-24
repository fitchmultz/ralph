//! Parallel run state persistence for crash recovery.
//!
//! Purpose:
//! - Parallel run state persistence for crash recovery.
//!
//! Responsibilities:
//! - Define the parallel state file format and helpers for direct-push mode.
//! - Persist and reload state for in-flight workers.
//!
//! Not handled here:
//! - Worker orchestration or process management (see `parallel/mod.rs`).
//! - Integration loop logic (see `worker.rs`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - State file lives at `.ralph/cache/parallel/state.json`.
//! - Callers update and persist state after each significant transition.
//! - Deserialization is tolerant of missing/unknown fields; callers normalize and persist the canonical shape.
//! - Schema version migrations are applied on load to ensure compatibility.

use crate::fsutil;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// =============================================================================
// Schema Version and Migration
// =============================================================================

/// Current parallel state schema version.
///
/// Version history:
/// - v1: Legacy schema with PR metadata and finished_without_pr
/// - v2: Minimal restart-safe schema with PR records and pending merges
/// - v3: Direct-push mode - worker lifecycle only, no PR/merge tracking
pub const PARALLEL_STATE_SCHEMA_VERSION: u32 = 3;

// =============================================================================
// Worker Lifecycle States
// =============================================================================

/// Lifecycle states for a parallel worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkerLifecycle {
    /// Worker is running task phases.
    #[default]
    Running,
    /// Worker is in the integration loop (rebase, conflict resolution, push).
    Integrating,
    /// Worker completed successfully (push succeeded).
    Completed,
    /// Worker failed with a terminal error.
    Failed,
    /// Push is blocked (conflicts, CI failure, or non-retryable error).
    BlockedPush,
}

/// A worker record tracking task execution and integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRecord {
    /// Task ID associated with this worker.
    pub task_id: String,
    /// Absolute path to the workspace directory.
    pub workspace_path: PathBuf,
    /// Current lifecycle state.
    #[serde(default)]
    pub lifecycle: WorkerLifecycle,
    /// Timestamp when the worker was started (RFC3339).
    pub started_at: String,
    /// Timestamp when the worker completed/failed (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    /// Number of push attempts made.
    #[serde(default)]
    pub push_attempts: u32,
    /// Last error message if failed/blocked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl WorkerRecord {
    pub fn new(task_id: impl Into<String>, workspace_path: PathBuf, started_at: String) -> Self {
        Self {
            task_id: task_id.into(),
            workspace_path,
            lifecycle: WorkerLifecycle::Running,
            started_at,
            completed_at: None,
            push_attempts: 0,
            last_error: None,
        }
    }

    /// Mark the worker as transitioning to integration phase.
    pub fn start_integration(&mut self) {
        self.lifecycle = WorkerLifecycle::Integrating;
    }

    /// Mark the worker as completed successfully.
    pub fn mark_completed(&mut self, timestamp: String) {
        self.lifecycle = WorkerLifecycle::Completed;
        self.completed_at = Some(timestamp);
    }

    /// Mark the worker as failed.
    pub fn mark_failed(&mut self, timestamp: String, error: impl Into<String>) {
        self.lifecycle = WorkerLifecycle::Failed;
        self.completed_at = Some(timestamp);
        self.last_error = Some(error.into());
    }

    /// Mark the worker as blocked on push.
    pub fn mark_blocked(&mut self, timestamp: String, error: impl Into<String>) {
        self.lifecycle = WorkerLifecycle::BlockedPush;
        self.completed_at = Some(timestamp);
        self.last_error = Some(error.into());
    }

    /// Increment push attempt counter.
    pub fn increment_push_attempt(&mut self) {
        self.push_attempts += 1;
    }

    /// Returns true if the worker is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.lifecycle,
            WorkerLifecycle::Completed | WorkerLifecycle::Failed | WorkerLifecycle::BlockedPush
        )
    }
}

// =============================================================================
// State File
// =============================================================================

/// Parallel state file for direct-push mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelStateFile {
    /// Schema version for migration compatibility.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Timestamp when the parallel run started (RFC3339).
    #[serde(default)]
    pub started_at: String,
    /// Target branch for direct pushes.
    #[serde(default)]
    pub target_branch: String,
    /// Active workers (one per task ID).
    #[serde(default)]
    pub workers: Vec<WorkerRecord>,
}

fn default_schema_version() -> u32 {
    1
}

impl ParallelStateFile {
    pub fn new(started_at: impl Into<String>, target_branch: impl Into<String>) -> Self {
        Self {
            schema_version: PARALLEL_STATE_SCHEMA_VERSION,
            started_at: started_at.into(),
            target_branch: target_branch.into(),
            workers: Vec::new(),
        }
    }

    /// Upsert a worker record. Replaces existing if task_id matches.
    pub fn upsert_worker(&mut self, record: WorkerRecord) {
        if let Some(existing) = self
            .workers
            .iter_mut()
            .find(|w| w.task_id == record.task_id)
        {
            *existing = record;
        } else {
            self.workers.push(record);
        }
    }

    /// Remove a worker by task_id.
    pub fn remove_worker(&mut self, task_id: &str) {
        self.workers.retain(|w| w.task_id != task_id);
    }

    /// Get a worker by task_id.
    pub fn get_worker(&self, task_id: &str) -> Option<&WorkerRecord> {
        self.workers.iter().find(|w| w.task_id == task_id)
    }

    /// Get a mutable worker by task_id.
    pub fn get_worker_mut(&mut self, task_id: &str) -> Option<&mut WorkerRecord> {
        self.workers.iter_mut().find(|w| w.task_id == task_id)
    }

    /// Returns true if there's a worker for this task_id.
    pub fn has_worker(&self, task_id: &str) -> bool {
        self.workers.iter().any(|w| w.task_id == task_id)
    }

    /// Get all workers in a specific lifecycle state.
    pub fn workers_by_lifecycle(
        &self,
        lifecycle: WorkerLifecycle,
    ) -> impl Iterator<Item = &WorkerRecord> {
        self.workers
            .iter()
            .filter(move |w| w.lifecycle == lifecycle)
    }

    /// Count workers that are not in a terminal state.
    pub fn active_worker_count(&self) -> usize {
        self.workers.iter().filter(|w| !w.is_terminal()).count()
    }

    /// Count workers in the blocked_push state.
    pub fn blocked_worker_count(&self) -> usize {
        self.workers_by_lifecycle(WorkerLifecycle::BlockedPush)
            .count()
    }
}

pub fn state_file_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph/cache/parallel/state.json")
}

/// Migrate legacy state to current schema version.
///
/// v1/v2 -> v3:
/// - Drop PR records, pending merges, tasks_in_flight
/// - Create fresh v3 state with empty workers list
fn migrate_state(mut state: ParallelStateFile) -> ParallelStateFile {
    if state.schema_version < PARALLEL_STATE_SCHEMA_VERSION {
        log::info!(
            "Migrating parallel state from schema v{} to v{}",
            state.schema_version,
            PARALLEL_STATE_SCHEMA_VERSION
        );
        // v3 is a clean break - we drop legacy fields and start fresh
        // Any in-flight work from v1/v2 is lost (should be handled by caller)
        state.schema_version = PARALLEL_STATE_SCHEMA_VERSION;
        state.workers.clear();
    }
    state
}

pub fn load_state(path: &Path) -> Result<Option<ParallelStateFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read parallel state {}", path.display()))?;
    let state: ParallelStateFile =
        crate::jsonc::parse_jsonc::<ParallelStateFile>(&raw, "parallel state")?;

    // Apply migrations
    let state = migrate_state(state);

    Ok(Some(state))
}

pub fn save_state(path: &Path, state: &ParallelStateFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parallel state dir {}", parent.display()))?;
    }
    let rendered = serde_json::to_string_pretty(state).context("serialize parallel state")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write parallel state {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "state/tests.rs"]
mod tests;
