//! Parallel run state persistence for crash recovery.
//!
//! Responsibilities:
//! - Define the parallel state file format and helpers for direct-push mode.
//! - Persist and reload state for in-flight workers.
//!
//! Not handled here:
//! - Worker orchestration or process management (see `parallel/mod.rs`).
//! - Integration loop logic (see `worker.rs`).
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
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // Schema Version and Migration Tests
    // =========================================================================

    #[test]
    fn new_state_has_current_schema_version() {
        let state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");
        assert_eq!(state.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
    }

    #[test]
    fn state_migration_v2_to_v3() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("state.json");

        // v2 state with legacy fields (will be dropped)
        let v2_state = r#"{
            "schema_version": 2,
            "started_at": "2026-02-01T00:00:00Z",
            "base_branch": "main",
            "merge_method": "squash",
            "merge_when": "as_created",
            "tasks_in_flight": [{"task_id": "RQ-0001", "workspace_path": "/tmp/ws", "branch": "b", "pid": 123}],
            "prs": [{"task_id": "RQ-0001", "pr_number": 5}],
            "pending_merges": [{"task_id": "RQ-0001", "pr_number": 5, "queued_at": "2026-02-01T00:00:00Z"}]
        }"#;

        std::fs::write(&path, v2_state)?;

        let state = load_state(&path)?.expect("state");
        assert_eq!(state.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
        // Workers should be empty after migration
        assert!(state.workers.is_empty());
        Ok(())
    }

    // =========================================================================
    // Worker Record Tests
    // =========================================================================

    #[test]
    fn worker_record_lifecycle_transitions() {
        let mut worker = WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/tmp/ws"),
            "2026-02-20T00:00:00Z".into(),
        );

        assert!(matches!(worker.lifecycle, WorkerLifecycle::Running));
        assert!(!worker.is_terminal());

        worker.start_integration();
        assert!(matches!(worker.lifecycle, WorkerLifecycle::Integrating));
        assert!(!worker.is_terminal());

        worker.mark_completed("2026-02-20T01:00:00Z".into());
        assert!(matches!(worker.lifecycle, WorkerLifecycle::Completed));
        assert!(worker.is_terminal());
        assert!(worker.completed_at.is_some());
    }

    #[test]
    fn worker_record_mark_failed() {
        let mut worker = WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/tmp/ws"),
            "2026-02-20T00:00:00Z".into(),
        );

        worker.mark_failed("2026-02-20T01:00:00Z".into(), "CI failed");

        assert!(matches!(worker.lifecycle, WorkerLifecycle::Failed));
        assert!(worker.is_terminal());
        assert_eq!(worker.last_error, Some("CI failed".into()));
    }

    #[test]
    fn worker_record_mark_blocked() {
        let mut worker = WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/tmp/ws"),
            "2026-02-20T00:00:00Z".into(),
        );

        worker.mark_blocked("2026-02-20T01:00:00Z".into(), "merge conflict");

        assert!(matches!(worker.lifecycle, WorkerLifecycle::BlockedPush));
        assert!(worker.is_terminal());
        assert_eq!(worker.last_error, Some("merge conflict".into()));
    }

    #[test]
    fn worker_record_push_attempts() {
        let mut worker = WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/tmp/ws"),
            "2026-02-20T00:00:00Z".into(),
        );

        assert_eq!(worker.push_attempts, 0);
        worker.increment_push_attempt();
        assert_eq!(worker.push_attempts, 1);
        worker.increment_push_attempt();
        assert_eq!(worker.push_attempts, 2);
    }

    // =========================================================================
    // State File Tests
    // =========================================================================

    #[test]
    fn state_upsert_worker_replaces_existing() {
        let mut state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");

        state.upsert_worker(WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/tmp/ws1"),
            "t1".into(),
        ));
        state.upsert_worker(WorkerRecord::new(
            "RQ-0002",
            PathBuf::from("/tmp/ws2"),
            "t2".into(),
        ));

        // Update RQ-0001 with new path
        let mut updated =
            WorkerRecord::new("RQ-0001", PathBuf::from("/tmp/ws1-new"), "t1-new".into());
        updated.start_integration();
        state.upsert_worker(updated);

        assert_eq!(state.workers.len(), 2);
        let w1 = state.get_worker("RQ-0001").unwrap();
        assert_eq!(w1.workspace_path, PathBuf::from("/tmp/ws1-new"));
        assert!(matches!(w1.lifecycle, WorkerLifecycle::Integrating));
    }

    #[test]
    fn state_remove_worker() {
        let mut state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");

        state.upsert_worker(WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/tmp/ws1"),
            "t1".into(),
        ));
        state.upsert_worker(WorkerRecord::new(
            "RQ-0002",
            PathBuf::from("/tmp/ws2"),
            "t2".into(),
        ));

        state.remove_worker("RQ-0001");

        assert_eq!(state.workers.len(), 1);
        assert!(state.get_worker("RQ-0001").is_none());
        assert!(state.get_worker("RQ-0002").is_some());
    }

    #[test]
    fn state_active_worker_count() {
        let mut state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");

        let w1 = WorkerRecord::new("RQ-0001", PathBuf::from("/tmp/ws1"), "t1".into());
        let mut w2 = WorkerRecord::new("RQ-0002", PathBuf::from("/tmp/ws2"), "t2".into());
        let mut w3 = WorkerRecord::new("RQ-0003", PathBuf::from("/tmp/ws3"), "t3".into());

        w2.mark_completed("t".into());
        w3.mark_blocked("t".into(), "error");

        state.upsert_worker(w1);
        state.upsert_worker(w2);
        state.upsert_worker(w3);

        // Only RQ-0001 is active (not terminal)
        assert_eq!(state.active_worker_count(), 1);
    }

    #[test]
    fn state_round_trips() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("state.json");

        let mut state = ParallelStateFile::new("2026-02-20T00:00:00Z", "main");
        let mut worker = WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/tmp/ws"),
            "2026-02-20T00:00:00Z".into(),
        );
        worker.start_integration();
        worker.increment_push_attempt();
        state.upsert_worker(worker);

        save_state(&path, &state)?;
        let loaded = load_state(&path)?.expect("state");

        assert_eq!(loaded.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
        assert_eq!(loaded.target_branch, "main");
        assert_eq!(loaded.workers.len(), 1);

        let w = &loaded.workers[0];
        assert_eq!(w.task_id, "RQ-0001");
        assert_eq!(w.workspace_path, PathBuf::from("/tmp/ws"));
        assert!(matches!(w.lifecycle, WorkerLifecycle::Integrating));
        assert_eq!(w.push_attempts, 1);

        Ok(())
    }

    #[test]
    fn state_deserialization_ignores_unknown_fields() -> Result<()> {
        let raw = r#"{
            "schema_version": 3,
            "started_at": "2026-02-20T00:00:00Z",
            "target_branch": "main",
            "unknown_top": "ignored",
            "workers": [{
                "task_id": "RQ-0001",
                "workspace_path": "/tmp/ws",
                "started_at": "2026-02-20T00:00:00Z",
                "unknown_worker": "ignored"
            }]
        }"#;

        let state: ParallelStateFile = serde_json::from_str(raw)?;
        assert_eq!(state.workers.len(), 1);
        assert_eq!(state.workers[0].task_id, "RQ-0001");

        Ok(())
    }
}
