//! Parallel run state persistence for crash recovery.
//!
//! Responsibilities:
//! - Define the parallel state file format and helpers.
//! - Persist and reload state for in-flight tasks, PRs, and pending merges.
//!
//! Not handled here:
//! - Worker orchestration or process management (see `parallel/mod.rs`).
//! - PR merge logic (see `merge_agent`).
//!
//! Invariants/assumptions:
//! - State file lives at `.ralph/cache/parallel/state.json`.
//! - Callers update and persist state after each significant transition.
//! - Deserialization is tolerant of missing/unknown fields; callers normalize and persist the canonical shape.
//! - Schema version migrations are applied on load to ensure compatibility.

use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};
use crate::fsutil;
use crate::git::WorkspaceSpec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// =============================================================================
// Schema Version and Migration
// =============================================================================

/// Current parallel state schema version.
///
/// Version history:
/// - v1: Legacy schema with finished_without_pr and full PR metadata
/// - v2: Minimal restart-safe schema (current)
pub const PARALLEL_STATE_SCHEMA_VERSION: u32 = 2;

// =============================================================================
// Pending Merge Job (new architecture - merge-agent subprocess)
// =============================================================================

/// Lifecycle states for a pending merge job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PendingMergeLifecycle {
    #[default]
    Queued,
    InProgress,
    RetryableFailed,
    TerminalFailed,
}

/// A merge job waiting to be processed or currently in progress.
///
/// This struct tracks merge jobs that are queued for the merge-agent subprocess.
/// The coordinator enqueues these after a worker succeeds and creates a PR,
/// then processes them via subprocess invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMergeJob {
    /// Task ID associated with this merge job.
    pub task_id: String,
    /// PR number to merge.
    pub pr_number: u32,
    /// Optional path to the workspace (for cleanup after merge).
    pub workspace_path: Option<PathBuf>,
    /// Current lifecycle state.
    #[serde(default)]
    pub lifecycle: PendingMergeLifecycle,
    /// Number of merge attempts (for retry policy).
    #[serde(default)]
    pub attempts: u8,
    /// Timestamp when queued (RFC3339).
    pub queued_at: String,
    /// Last error message if failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelStateFile {
    /// Schema version for migration compatibility.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub base_branch: String,
    #[serde(default)]
    pub merge_method: ParallelMergeMethod,
    #[serde(default)]
    pub merge_when: ParallelMergeWhen,
    /// Active workers (one per task ID).
    #[serde(default)]
    pub tasks_in_flight: Vec<ParallelTaskRecord>,
    /// PR records (simplified to minimal fields).
    #[serde(default)]
    pub prs: Vec<ParallelPrRecord>,
    /// Merge jobs queued or in-progress (new architecture using merge-agent subprocess).
    #[serde(default)]
    pub pending_merges: Vec<PendingMergeJob>,
}

fn default_schema_version() -> u32 {
    1
} // v1 = legacy

impl ParallelStateFile {
    pub fn new(
        started_at: String,
        base_branch: String,
        merge_method: ParallelMergeMethod,
        merge_when: ParallelMergeWhen,
    ) -> Self {
        Self {
            schema_version: PARALLEL_STATE_SCHEMA_VERSION,
            started_at,
            base_branch,
            merge_method,
            merge_when,
            tasks_in_flight: Vec::new(),
            prs: Vec::new(),
            pending_merges: Vec::new(),
        }
    }

    pub fn upsert_task(&mut self, record: ParallelTaskRecord) {
        if let Some(existing) = self
            .tasks_in_flight
            .iter_mut()
            .find(|item| item.task_id == record.task_id)
        {
            *existing = record;
        } else {
            self.tasks_in_flight.push(record);
        }
    }

    pub fn remove_task(&mut self, task_id: &str) {
        self.tasks_in_flight.retain(|item| item.task_id != task_id);
    }

    pub fn upsert_pr(&mut self, record: ParallelPrRecord) {
        if let Some(existing) = self
            .prs
            .iter_mut()
            .find(|item| item.task_id == record.task_id)
        {
            *existing = record;
        } else {
            self.prs.push(record);
        }
    }

    pub fn mark_pr_merged(&mut self, task_id: &str) {
        if let Some(existing) = self.prs.iter_mut().find(|item| item.task_id == task_id) {
            existing.lifecycle = ParallelPrLifecycle::Merged;
        }
    }

    pub fn has_pr_record(&self, task_id: &str) -> bool {
        self.prs.iter().any(|item| item.task_id == task_id)
    }

    // =========================================================================
    // Pending Merge Job Management (new architecture - merge-agent subprocess)
    // =========================================================================

    /// Queue a new merge job after worker success.
    ///
    /// If a job for this task already exists, it is replaced with the new one.
    pub fn enqueue_merge(&mut self, job: PendingMergeJob) {
        // Remove any existing entry for this task
        self.pending_merges.retain(|j| j.task_id != job.task_id);
        self.pending_merges.push(job);
    }

    /// Get the next queued merge job (FIFO order).
    pub fn next_queued_merge(&self) -> Option<&PendingMergeJob> {
        self.pending_merges
            .iter()
            .find(|j| j.lifecycle == PendingMergeLifecycle::Queued)
    }

    /// Get the next queued merge job mutably (FIFO order).
    pub fn next_queued_merge_mut(&mut self) -> Option<&mut PendingMergeJob> {
        self.pending_merges
            .iter_mut()
            .find(|j| j.lifecycle == PendingMergeLifecycle::Queued)
    }

    /// Mark a merge job as in-progress.
    pub fn mark_merge_in_progress(&mut self, task_id: &str) {
        if let Some(job) = self
            .pending_merges
            .iter_mut()
            .find(|j| j.task_id == task_id)
        {
            job.lifecycle = PendingMergeLifecycle::InProgress;
        }
    }

    /// Update merge job after completion or failure.
    ///
    /// On success, the job will be marked for removal (lifecycle set to a marker).
    /// On failure, attempts are incremented and lifecycle is set appropriately.
    pub fn update_merge_result(
        &mut self,
        task_id: &str,
        success: bool,
        error: Option<String>,
        retryable: bool,
    ) {
        if let Some(job) = self
            .pending_merges
            .iter_mut()
            .find(|j| j.task_id == task_id)
        {
            if success {
                // Mark for removal - caller should call remove_pending_merge
                job.lifecycle = PendingMergeLifecycle::Queued; // marker for removal
            } else {
                job.attempts += 1;
                job.lifecycle = if retryable {
                    PendingMergeLifecycle::RetryableFailed
                } else {
                    PendingMergeLifecycle::TerminalFailed
                };
                job.last_error = error;
            }
        }
    }

    /// Set a pending merge back to queued state (for retry).
    pub fn requeue_merge(&mut self, task_id: &str) {
        if let Some(job) = self
            .pending_merges
            .iter_mut()
            .find(|j| j.task_id == task_id)
        {
            job.lifecycle = PendingMergeLifecycle::Queued;
        }
    }

    /// Mark a merge job as terminally failed.
    pub fn mark_merge_terminal_failed(&mut self, task_id: &str, error: String) {
        if let Some(job) = self
            .pending_merges
            .iter_mut()
            .find(|j| j.task_id == task_id)
        {
            job.lifecycle = PendingMergeLifecycle::TerminalFailed;
            job.last_error = Some(error);
        }
    }

    /// Remove a completed merge job.
    pub fn remove_pending_merge(&mut self, task_id: &str) {
        self.pending_merges.retain(|j| j.task_id != task_id);
    }

    /// Count pending merges (for capacity tracking).
    pub fn pending_merge_count(&self) -> usize {
        self.pending_merges.len()
    }

    /// Check if there are any queued merges waiting to be processed.
    pub fn has_queued_merges(&self) -> bool {
        self.pending_merges
            .iter()
            .any(|j| j.lifecycle == PendingMergeLifecycle::Queued)
    }

    /// Get a pending merge job by task_id.
    pub fn get_pending_merge(&self, task_id: &str) -> Option<&PendingMergeJob> {
        self.pending_merges.iter().find(|j| j.task_id == task_id)
    }

    /// Get a mutable pending merge job by task_id.
    pub fn get_pending_merge_mut(&mut self, task_id: &str) -> Option<&mut PendingMergeJob> {
        self.pending_merges
            .iter_mut()
            .find(|j| j.task_id == task_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelTaskRecord {
    pub task_id: String,
    #[serde(alias = "worktree_path")]
    pub workspace_path: String,
    pub branch: String,
    pub pid: Option<u32>,

    /// Timestamp when the task was started (RFC3339).
    /// Backward compatible: legacy state files may omit this field.
    #[serde(default)]
    pub started_at: String,
}

impl ParallelTaskRecord {
    pub(crate) fn new(
        task_id: &str,
        workspace: &WorkspaceSpec,
        pid: u32,
        started_at: Option<String>,
    ) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_path: workspace.path.to_string_lossy().to_string(),
            branch: workspace.branch.clone(),
            pid: Some(pid),
            started_at: started_at.unwrap_or_else(crate::timeutil::now_utc_rfc3339_or_fallback),
        }
    }
}

/// PR lifecycle state for persisted parallel PR records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ParallelPrLifecycle {
    #[default]
    Open,
    Closed,
    Merged,
}

/// Minimal PR record for restart-safe state.
///
/// Only tracks what's needed for:
/// - Capacity tracking on resume (is there an open PR for this task?)
/// - Merge-agent invocation (pr_number)
/// - PR lifecycle sync on startup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelPrRecord {
    /// Task ID associated with this PR.
    pub task_id: String,
    /// PR number for merge-agent and GitHub queries.
    pub pr_number: u32,
    /// PR lifecycle state (synced from GitHub on startup).
    #[serde(default)]
    pub lifecycle: ParallelPrLifecycle,
}

impl ParallelPrRecord {
    pub(crate) fn new(
        task_id: &str,
        pr: &crate::git::PrInfo,
        _workspace_path: Option<&Path>,
    ) -> Self {
        Self {
            task_id: task_id.to_string(),
            pr_number: pr.number,
            lifecycle: ParallelPrLifecycle::Open,
        }
    }

    /// Returns true if the PR is open (not merged/closed).
    /// These represent work already in flight from a prior run that should
    /// count toward max_tasks limits on resume.
    pub fn is_open_unmerged(&self) -> bool {
        matches!(self.lifecycle, ParallelPrLifecycle::Open)
    }
}

pub fn state_file_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph/cache/parallel/state.json")
}

/// Migrate legacy state to current schema version.
///
/// v1 -> v2:
/// - Clear finished_without_pr on load (will be recomputed dynamically if needed)
/// - Simplify PR records (excess fields are ignored by serde defaults)
fn migrate_state(mut state: ParallelStateFile) -> ParallelStateFile {
    if state.schema_version < 2 {
        log::info!(
            "Migrating parallel state from schema v{} to v{}",
            state.schema_version,
            PARALLEL_STATE_SCHEMA_VERSION
        );
        // v1 -> v2: Clear finished_without_pr on load (no longer persisted)
        // The field is no longer part of the schema, so no action needed here
        // since deserialization won't populate it.
        state.schema_version = PARALLEL_STATE_SCHEMA_VERSION;
    }
    state
}

pub fn load_state(path: &Path) -> Result<Option<ParallelStateFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read parallel state {}", path.display()))?;
    let state = crate::jsonc::parse_jsonc::<ParallelStateFile>(&raw, "parallel state")?;

    // Apply migrations
    let state = migrate_state(state);

    Ok(Some(state))
}

pub(crate) fn save_state(path: &Path, state: &ParallelStateFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parallel state dir {}", parent.display()))?;
    }
    let rendered = serde_json::to_string_pretty(state).context("serialize parallel state")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write parallel state {}", path.display()))?;
    Ok(())
}

/// Summary of PR reconciliation results.
#[derive(Debug, Clone, Default)]
pub(crate) struct ReconcileSummary {
    pub open_count: usize,
    pub closed_count: usize,
    pub merged_count: usize,
    pub unknown_count: usize,
    pub error_count: usize,
    pub affected_task_ids: Vec<String>,
}

impl ReconcileSummary {
    pub fn has_changes(&self) -> bool {
        !self.affected_task_ids.is_empty()
    }
}

/// Reconcile persisted PR records against current GitHub state.
///
/// For each PR record where `lifecycle == Open`,
/// queries GitHub to determine if the PR is still open. Updates the
/// record's lifecycle based on the current state.
///
/// Errors during individual PR lookups are logged as warnings and do not
/// abort the reconciliation process.
pub(crate) fn reconcile_pr_records(
    repo_root: &Path,
    state_file: &mut ParallelStateFile,
) -> Result<ReconcileSummary> {
    use crate::git;

    let mut summary = ReconcileSummary::default();

    for record in state_file.prs.iter_mut() {
        // Skip already closed/merged records
        if !matches!(record.lifecycle, ParallelPrLifecycle::Open) {
            match record.lifecycle {
                ParallelPrLifecycle::Open => summary.open_count += 1,
                ParallelPrLifecycle::Closed => summary.closed_count += 1,
                ParallelPrLifecycle::Merged => summary.merged_count += 1,
            }
            continue;
        }

        match git::pr_lifecycle_status(repo_root, record.pr_number) {
            Ok(status) => {
                match status.lifecycle {
                    git::PrLifecycle::Open => {
                        record.lifecycle = ParallelPrLifecycle::Open;
                        summary.open_count += 1;
                    }
                    git::PrLifecycle::Closed => {
                        record.lifecycle = ParallelPrLifecycle::Closed;
                        summary.closed_count += 1;
                        summary.affected_task_ids.push(record.task_id.clone());
                    }
                    git::PrLifecycle::Merged => {
                        record.lifecycle = ParallelPrLifecycle::Merged;
                        summary.merged_count += 1;
                        summary.affected_task_ids.push(record.task_id.clone());
                    }
                    git::PrLifecycle::Unknown(ref s) => {
                        // Treat unknown as blocking (keep as Open)
                        log::warn!(
                            "PR {} for task {} has unknown lifecycle state '{}'; treating as blocking",
                            record.pr_number,
                            record.task_id,
                            s
                        );
                        record.lifecycle = ParallelPrLifecycle::Open;
                        summary.unknown_count += 1;
                    }
                }
            }
            Err(err) => {
                // Log warning and keep record as blocking
                log::warn!(
                    "Failed to query PR {} for task {}: {}; keeping as blocking",
                    record.pr_number,
                    record.task_id,
                    err
                );
                summary.error_count += 1;
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};
    use tempfile::TempDir;

    // =========================================================================
    // Schema Version and Migration Tests
    // =========================================================================

    #[test]
    fn state_migration_v1_to_v2() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("state.json");

        // v1 state with finished_without_pr entries (which are now ignored)
        let v1_state = r#"{
            "schema_version": 1,
            "started_at": "2026-02-01T00:00:00Z",
            "base_branch": "main",
            "merge_method": "squash",
            "merge_when": "as_created",
            "tasks_in_flight": [],
            "prs": [],
            "finished_without_pr": [
                {"task_id": "RQ-0001", "workspace_path": "/tmp/ws", "branch": "b", "success": true, "finished_at": "2026-02-01T00:00:00Z"}
            ],
            "pending_merges": []
        }"#;

        std::fs::write(&path, v1_state)?;

        let state = load_state(&path)?.expect("state");
        assert_eq!(state.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn state_deserialization_accepts_legacy_pr_fields() -> Result<()> {
        // Ensure we can load old state files with extra PR fields
        let raw = r#"{
            "schema_version": 1,
            "started_at": "2026-02-01T00:00:00Z",
            "base_branch": "main",
            "merge_method": "squash",
            "merge_when": "as_created",
            "tasks_in_flight": [],
            "prs": [{
                "task_id": "RQ-0001",
                "pr_number": 5,
                "pr_url": "https://example.com/pr/5",
                "head": "ralph/RQ-0001",
                "base": "main",
                "workspace_path": "/tmp/ws",
                "merged": false,
                "lifecycle": "open",
                "merge_blocker": "some blocker"
            }],
            "pending_merges": []
        }"#;

        let state: ParallelStateFile = serde_json::from_str(raw)?;
        assert_eq!(state.prs.len(), 1);
        assert_eq!(state.prs[0].task_id, "RQ-0001");
        assert_eq!(state.prs[0].pr_number, 5);
        // Excess fields are silently ignored via serde defaults
        Ok(())
    }

    #[test]
    fn one_active_worker_per_task_invariant() {
        let mut state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state.upsert_task(ParallelTaskRecord {
            task_id: "RQ-0001".into(),
            workspace_path: "/tmp/ws1".into(),
            branch: "ralph/RQ-0001".into(),
            pid: Some(123),
            started_at: "2026-02-01T00:00:00Z".into(),
        });

        // Upsert same task_id should replace, not duplicate
        state.upsert_task(ParallelTaskRecord {
            task_id: "RQ-0001".into(),
            workspace_path: "/tmp/ws2".into(),
            branch: "ralph/RQ-0001".into(),
            pid: Some(456),
            started_at: "2026-02-01T00:01:00Z".into(),
        });

        assert_eq!(state.tasks_in_flight.len(), 1);
        assert_eq!(state.tasks_in_flight[0].pid, Some(456));
    }

    #[test]
    fn one_pending_merge_per_task_invariant() {
        let mut state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 1,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 0,
            queued_at: "2026-02-01T00:00:00Z".into(),
            last_error: None,
        });

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 2, // Updated PR
            workspace_path: Some(PathBuf::from("/tmp/ws")),
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 1,
            queued_at: "2026-02-01T01:00:00Z".into(),
            last_error: None,
        });

        assert_eq!(state.pending_merges.len(), 1);
        assert_eq!(state.pending_merges[0].pr_number, 2);
    }

    #[test]
    fn new_state_has_current_schema_version() {
        let state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        assert_eq!(state.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
    }

    // =========================================================================
    // Basic State Round-Trip Tests
    // =========================================================================

    #[test]
    fn state_round_trips() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("state.json");
        let mut state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.upsert_pr(ParallelPrRecord {
            task_id: "RQ-0001".to_string(),
            pr_number: 5,
            lifecycle: ParallelPrLifecycle::Open,
        });

        save_state(&path, &state)?;
        let loaded = load_state(&path)?.expect("state");
        assert_eq!(loaded.base_branch, "main");
        assert_eq!(loaded.prs.len(), 1);
        assert_eq!(loaded.schema_version, PARALLEL_STATE_SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn state_deserialization_accepts_legacy_worktree_path_in_tasks() -> Result<()> {
        let raw = r#"{
            "started_at":"2026-02-01T00:00:00Z",
            "base_branch":"main",
            "merge_method":"squash",
            "merge_when":"as_created",
            "tasks_in_flight":[{"task_id":"RQ-0001","worktree_path":"/tmp/wt","branch":"b","pid":1}],
            "prs":[]
        }"#;
        let state: ParallelStateFile = serde_json::from_str(raw)?;
        assert_eq!(state.tasks_in_flight.len(), 1);
        assert_eq!(state.tasks_in_flight[0].workspace_path, "/tmp/wt");
        Ok(())
    }

    #[test]
    fn state_deserialization_ignores_unknown_fields() -> Result<()> {
        let raw = r#"{
            "started_at":"2026-02-01T00:00:00Z",
            "base_branch":"main",
            "merge_method":"squash",
            "merge_when":"as_created",
            "extra_top":"ignored",
            "tasks_in_flight":[{"task_id":"RQ-0001","workspace_path":"/tmp/wt","branch":"b","pid":1,"extra_task":true}],
            "prs":[{"task_id":"RQ-0002","pr_number":5,"pr_url":"https://example.com/pr/5","merged":false,"extra_pr":"ignored"}],
            "finished_without_pr":[{"task_id":"RQ-0003","workspace_path":"/tmp/wt","branch":"b","success":true,"finished_at":"2026-02-01T00:00:00Z","extra_blocker":"ignored"}]
        }"#;
        let state: ParallelStateFile = serde_json::from_str(raw)?;
        assert_eq!(state.tasks_in_flight.len(), 1);
        assert_eq!(state.prs.len(), 1);
        Ok(())
    }

    #[test]
    fn state_deserialization_allows_missing_base_branch() -> Result<()> {
        let raw = r#"{
            "merge_method":"squash",
            "merge_when":"as_created",
            "tasks_in_flight":[],
            "prs":[]
        }"#;
        let state: ParallelStateFile = serde_json::from_str(raw)?;
        assert!(state.base_branch.is_empty());
        assert!(state.started_at.is_empty());
        Ok(())
    }

    #[test]
    fn pr_lifecycle_defaults_to_open() {
        // Verify backward compatibility: old state files without lifecycle default to Open
        let raw = r#"{
            "task_id":"RQ-0001",
            "pr_number":5
        }"#;
        let record: ParallelPrRecord = serde_json::from_str(raw).unwrap();
        assert!(matches!(record.lifecycle, ParallelPrLifecycle::Open));
    }

    #[test]
    fn pr_lifecycle_round_trips() {
        let record = ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 10,
            lifecycle: ParallelPrLifecycle::Merged,
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: ParallelPrRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.lifecycle, ParallelPrLifecycle::Merged));
    }

    // Tests for reconcile_pr_records with stubbed gh binary
    use crate::testsupport::path::with_prepend_path;
    use std::io::Write;

    fn create_fake_gh(tmp_dir: &TempDir, pr_responses: &[(u32, &str)]) -> PathBuf {
        let bin_dir = tmp_dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let gh_path = bin_dir.join("gh");

        let mut script = String::from(
            r#"#!/bin/bash
# Fake gh script for testing
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
    PR_NUM="$3"
"#,
        );

        for (pr_num, response) in pr_responses {
            script.push_str(&format!(
                r#"
    if [ "$PR_NUM" = "{}" ]; then
        echo '{}'
        exit 0
    fi
"#,
                pr_num, response
            ));
        }

        script.push_str(
            r#"
fi
echo "Unknown PR or command" >&2
exit 1
"#,
        );

        let mut file = std::fs::File::create(&gh_path).unwrap();
        file.write_all(script.as_bytes()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = file.metadata().unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&gh_path, perms).unwrap();
        }
        bin_dir
    }

    #[test]
    fn reconcile_pr_records_updates_open_closed_merged() -> Result<()> {
        let temp = TempDir::new()?;

        // PR 1 stays OPEN
        // PR 2 is CLOSED (not merged)
        // PR 3 is MERGED
        let responses = vec![
            (
                1,
                r#"{"state":"OPEN","merged":false,"mergeStateStatus":"CLEAN","number":1,"url":"https://example.com/pr/1","headRefName":"ralph/RQ-0001","baseRefName":"main","isDraft":false}"#,
            ),
            (
                2,
                r#"{"state":"CLOSED","merged":false,"mergeStateStatus":"CLEAN","number":2,"url":"https://example.com/pr/2","headRefName":"ralph/RQ-0002","baseRefName":"main","isDraft":false}"#,
            ),
            (
                3,
                r#"{"state":"CLOSED","merged":true,"mergeStateStatus":"CLEAN","number":3,"url":"https://example.com/pr/3","headRefName":"ralph/RQ-0003","baseRefName":"main","isDraft":false}"#,
            ),
        ];
        let bin_dir = create_fake_gh(&temp, &responses);

        let result = with_prepend_path(&bin_dir, || {
            let mut state_file = ParallelStateFile::new(
                "2026-02-01T00:00:00Z".to_string(),
                "main".to_string(),
                ParallelMergeMethod::Squash,
                ParallelMergeWhen::AsCreated,
            );

            // Add 3 PR records, all initially Open
            state_file.upsert_pr(ParallelPrRecord {
                task_id: "RQ-0001".to_string(),
                pr_number: 1,
                lifecycle: ParallelPrLifecycle::Open,
            });
            state_file.upsert_pr(ParallelPrRecord {
                task_id: "RQ-0002".to_string(),
                pr_number: 2,
                lifecycle: ParallelPrLifecycle::Open,
            });
            state_file.upsert_pr(ParallelPrRecord {
                task_id: "RQ-0003".to_string(),
                pr_number: 3,
                lifecycle: ParallelPrLifecycle::Open,
            });

            reconcile_pr_records(temp.path(), &mut state_file).map(|s| (s, state_file))
        });

        let (summary, state_file) = result?;

        // Assert summary
        assert!(summary.has_changes());
        assert_eq!(summary.open_count, 1);
        assert_eq!(summary.closed_count, 1);
        assert_eq!(summary.merged_count, 1);
        assert_eq!(summary.affected_task_ids.len(), 2); // RQ-0002 and RQ-0003
        assert!(summary.affected_task_ids.contains(&"RQ-0002".to_string()));
        assert!(summary.affected_task_ids.contains(&"RQ-0003".to_string()));

        // Assert state file updates
        let pr1 = state_file
            .prs
            .iter()
            .find(|p| p.task_id == "RQ-0001")
            .unwrap();
        let pr2 = state_file
            .prs
            .iter()
            .find(|p| p.task_id == "RQ-0002")
            .unwrap();
        let pr3 = state_file
            .prs
            .iter()
            .find(|p| p.task_id == "RQ-0003")
            .unwrap();

        assert!(matches!(pr1.lifecycle, ParallelPrLifecycle::Open));
        assert!(matches!(pr2.lifecycle, ParallelPrLifecycle::Closed));
        assert!(matches!(pr3.lifecycle, ParallelPrLifecycle::Merged));

        Ok(())
    }

    #[test]
    fn reconcile_pr_records_handles_gh_errors_gracefully() -> Result<()> {
        let temp = TempDir::new()?;

        // Fake gh that fails for PR 2
        let responses = vec![
            (
                1,
                r#"{"state":"OPEN","merged":false,"mergeStateStatus":"CLEAN","number":1,"url":"https://example.com/pr/1","headRefName":"ralph/RQ-0001","baseRefName":"main","isDraft":false}"#,
            ),
            // PR 2 will fail (not in the response list)
        ];
        let bin_dir = create_fake_gh(&temp, &responses);

        let result = with_prepend_path(&bin_dir, || {
            let mut state_file = ParallelStateFile::new(
                "2026-02-01T00:00:00Z".to_string(),
                "main".to_string(),
                ParallelMergeMethod::Squash,
                ParallelMergeWhen::AsCreated,
            );

            state_file.upsert_pr(ParallelPrRecord {
                task_id: "RQ-0001".to_string(),
                pr_number: 1,
                lifecycle: ParallelPrLifecycle::Open,
            });
            state_file.upsert_pr(ParallelPrRecord {
                task_id: "RQ-0002".to_string(),
                pr_number: 2,
                lifecycle: ParallelPrLifecycle::Open,
            });

            reconcile_pr_records(temp.path(), &mut state_file).map(|s| (s, state_file))
        });
        let (summary, state_file) = result?;

        // Should not fail, but should report error for PR 2
        assert_eq!(summary.error_count, 1);
        assert_eq!(summary.open_count, 1); // PR 1 stayed open

        // PR 2 should remain unchanged (still blocking)
        let pr2 = state_file
            .prs
            .iter()
            .find(|p| p.task_id == "RQ-0002")
            .unwrap();
        assert!(matches!(pr2.lifecycle, ParallelPrLifecycle::Open));

        Ok(())
    }

    // =========================================================================
    // Pending Merge Job Tests
    // =========================================================================

    #[test]
    fn pending_merge_job_serialization() {
        let job = PendingMergeJob {
            task_id: "RQ-0001".to_string(),
            pr_number: 42,
            workspace_path: Some(PathBuf::from("/tmp/ws")),
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 0,
            queued_at: "2026-02-17T00:00:00Z".to_string(),
            last_error: None,
        };
        let json = serde_json::to_string(&job).unwrap();
        let parsed: PendingMergeJob = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "RQ-0001");
        assert_eq!(parsed.pr_number, 42);
        assert_eq!(parsed.workspace_path, Some(PathBuf::from("/tmp/ws")));
        assert_eq!(parsed.lifecycle, PendingMergeLifecycle::Queued);
    }

    #[test]
    fn pending_merge_job_lifecycle_defaults_to_queued() {
        let raw = r#"{
            "task_id": "RQ-0002",
            "pr_number": 1,
            "queued_at": "2026-02-17T00:00:00Z"
        }"#;
        let job: PendingMergeJob = serde_json::from_str(raw).unwrap();
        assert_eq!(job.lifecycle, PendingMergeLifecycle::Queued);
        assert_eq!(job.attempts, 0);
        assert!(job.last_error.is_none());
    }

    #[test]
    fn state_file_enqueue_merge() {
        let mut state = ParallelStateFile::new(
            "2026-02-17T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 1,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 0,
            queued_at: "2026-02-17T00:00:00Z".into(),
            last_error: None,
        });

        assert_eq!(state.pending_merges.len(), 1);
        assert!(state.next_queued_merge().is_some());
    }

    #[test]
    fn state_file_merge_lifecycle_transitions() {
        let mut state = ParallelStateFile::new(
            "2026-02-17T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 1,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 0,
            queued_at: "2026-02-17T00:00:00Z".into(),
            last_error: None,
        });

        // Mark in-progress
        state.mark_merge_in_progress("RQ-0001");
        assert_eq!(
            state.pending_merges[0].lifecycle,
            PendingMergeLifecycle::InProgress
        );

        // Update with retryable failure
        state.update_merge_result("RQ-0001", false, Some("conflict".into()), true);
        assert_eq!(
            state.pending_merges[0].lifecycle,
            PendingMergeLifecycle::RetryableFailed
        );
        assert_eq!(state.pending_merges[0].attempts, 1);
        assert_eq!(state.pending_merges[0].last_error, Some("conflict".into()));

        // Requeue for retry
        state.requeue_merge("RQ-0001");
        assert_eq!(
            state.pending_merges[0].lifecycle,
            PendingMergeLifecycle::Queued
        );

        // Remove on success
        state.remove_pending_merge("RQ-0001");
        assert!(state.pending_merges.is_empty());
    }

    #[test]
    fn state_file_update_merge_result_success() {
        let mut state = ParallelStateFile::new(
            "2026-02-17T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 1,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::InProgress,
            attempts: 2,
            queued_at: "2026-02-17T00:00:00Z".into(),
            last_error: Some("previous error".into()),
        });

        // Update with success
        state.update_merge_result("RQ-0001", true, None, false);

        // On success, lifecycle is set to Queued as a marker for removal
        let job = state.get_pending_merge("RQ-0001").unwrap();
        assert_eq!(job.lifecycle, PendingMergeLifecycle::Queued);
    }

    #[test]
    fn state_file_update_merge_result_terminal_failure() {
        let mut state = ParallelStateFile::new(
            "2026-02-17T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 1,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::InProgress,
            attempts: 0,
            queued_at: "2026-02-17T00:00:00Z".into(),
            last_error: None,
        });

        // Update with terminal failure
        state.update_merge_result("RQ-0001", false, Some("PR closed".into()), false);

        let job = state.get_pending_merge("RQ-0001").unwrap();
        assert_eq!(job.lifecycle, PendingMergeLifecycle::TerminalFailed);
        assert_eq!(job.last_error, Some("PR closed".into()));
    }

    #[test]
    fn state_file_pending_merge_count() {
        let mut state = ParallelStateFile::new(
            "2026-02-17T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        assert_eq!(state.pending_merge_count(), 0);

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 1,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 0,
            queued_at: "2026-02-17T00:00:00Z".into(),
            last_error: None,
        });

        assert_eq!(state.pending_merge_count(), 1);

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0002".into(),
            pr_number: 2,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 0,
            queued_at: "2026-02-17T00:00:00Z".into(),
            last_error: None,
        });

        assert_eq!(state.pending_merge_count(), 2);
    }

    #[test]
    fn state_file_has_queued_merges() {
        let mut state = ParallelStateFile::new(
            "2026-02-17T00:00:00Z".into(),
            "main".into(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        assert!(!state.has_queued_merges());

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 1,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::InProgress,
            attempts: 0,
            queued_at: "2026-02-17T00:00:00Z".into(),
            last_error: None,
        });

        // InProgress should not count as queued
        assert!(!state.has_queued_merges());

        state.requeue_merge("RQ-0001");
        assert!(state.has_queued_merges());
    }

    #[test]
    fn state_file_round_trips_with_pending_merges() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("state.json");
        let mut state = ParallelStateFile::new(
            "2026-02-17T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state.enqueue_merge(PendingMergeJob {
            task_id: "RQ-0001".into(),
            pr_number: 42,
            workspace_path: Some(PathBuf::from("/tmp/ws/RQ-0001")),
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 1,
            queued_at: "2026-02-17T00:00:00Z".into(),
            last_error: Some("previous attempt failed".into()),
        });

        save_state(&path, &state)?;
        let loaded = load_state(&path)?.expect("state should exist");

        assert_eq!(loaded.pending_merges.len(), 1);
        let job = &loaded.pending_merges[0];
        assert_eq!(job.task_id, "RQ-0001");
        assert_eq!(job.pr_number, 42);
        assert_eq!(job.attempts, 1);
        assert_eq!(job.last_error, Some("previous attempt failed".into()));
        Ok(())
    }
}
