//! Parallel run state persistence for crash recovery.
//!
//! Responsibilities:
//! - Define the parallel state file format and helpers.
//! - Persist and reload state for in-flight tasks, PRs, and finished-without-PR blockers.
//!
//! Not handled here:
//! - Worker orchestration or process management (see `parallel/mod.rs`).
//! - PR merge logic (see `merge_runner`).
//!
//! Invariants/assumptions:
//! - State file lives at `.ralph/cache/parallel/state.json`.
//! - Callers update and persist state after each significant transition.
//! - Deserialization is tolerant of missing/unknown fields; callers normalize and persist the canonical shape.

use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};
use crate::fsutil;
use crate::git::WorkspaceSpec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ParallelStateFile {
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub base_branch: String,
    #[serde(default)]
    pub merge_method: ParallelMergeMethod,
    #[serde(default)]
    pub merge_when: ParallelMergeWhen,
    #[serde(default)]
    pub tasks_in_flight: Vec<ParallelTaskRecord>,
    #[serde(default)]
    pub prs: Vec<ParallelPrRecord>,
    #[serde(default)]
    pub finished_without_pr: Vec<ParallelFinishedWithoutPrRecord>,
}

impl ParallelStateFile {
    pub fn new(
        started_at: String,
        base_branch: String,
        merge_method: ParallelMergeMethod,
        merge_when: ParallelMergeWhen,
    ) -> Self {
        Self {
            started_at,
            base_branch,
            merge_method,
            merge_when,
            tasks_in_flight: Vec::new(),
            prs: Vec::new(),
            finished_without_pr: Vec::new(),
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
        self.remove_finished_without_pr(&record.task_id);
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
            existing.merged = true;
            existing.lifecycle = ParallelPrLifecycle::Merged;
        }
    }

    pub fn has_pr_record(&self, task_id: &str) -> bool {
        self.prs.iter().any(|item| item.task_id == task_id)
    }

    pub fn upsert_finished_without_pr(&mut self, record: ParallelFinishedWithoutPrRecord) {
        if let Some(existing) = self
            .finished_without_pr
            .iter_mut()
            .find(|item| item.task_id == record.task_id)
        {
            *existing = record;
        } else {
            self.finished_without_pr.push(record);
        }
    }

    pub fn remove_finished_without_pr(&mut self, task_id: &str) -> bool {
        let before = self.finished_without_pr.len();
        self.finished_without_pr
            .retain(|item| item.task_id != task_id);
        before != self.finished_without_pr.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ParallelTaskRecord {
    pub task_id: String,
    #[serde(alias = "worktree_path")]
    pub workspace_path: String,
    pub branch: String,
    pub pid: Option<u32>,
}

impl ParallelTaskRecord {
    pub fn new(task_id: &str, workspace: &WorkspaceSpec, pid: u32) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_path: workspace.path.to_string_lossy().to_string(),
            branch: workspace.branch.clone(),
            pid: Some(pid),
        }
    }
}

/// PR lifecycle state for persisted parallel PR records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub(crate) enum ParallelPrLifecycle {
    #[default]
    Open,
    Closed,
    Merged,
}

/// Reason a parallel task finished without a PR record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub(crate) enum ParallelNoPrReason {
    #[default]
    Unknown,
    AutoPrDisabled,
    PrCreateFailed,
    DraftPrDisabled,
    DraftPrSkippedNoChanges,
}

impl ParallelNoPrReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParallelNoPrReason::Unknown => "unknown",
            ParallelNoPrReason::AutoPrDisabled => "auto_pr_disabled",
            ParallelNoPrReason::PrCreateFailed => "pr_create_failed",
            ParallelNoPrReason::DraftPrDisabled => "draft_pr_disabled",
            ParallelNoPrReason::DraftPrSkippedNoChanges => "draft_pr_skipped_no_changes",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ParallelPrRecord {
    pub task_id: String,
    pub pr_number: u32,
    pub pr_url: String,
    #[serde(default)]
    pub head: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default, alias = "worktree_path")]
    pub workspace_path: Option<String>,
    pub merged: bool,
    #[serde(default)]
    pub lifecycle: ParallelPrLifecycle,
}

impl ParallelPrRecord {
    pub fn new(task_id: &str, pr: &crate::git::PrInfo, workspace_path: Option<&Path>) -> Self {
        Self {
            task_id: task_id.to_string(),
            pr_number: pr.number,
            pr_url: pr.url.clone(),
            head: Some(pr.head.clone()),
            base: Some(pr.base.clone()),
            workspace_path: workspace_path.map(|p| p.to_string_lossy().to_string()),
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
        }
    }

    pub fn pr_info(&self, fallback_head: &str, fallback_base: &str) -> crate::git::PrInfo {
        let head = self
            .head
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_head)
            .to_string();
        let base = self
            .base
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_base)
            .to_string();
        crate::git::PrInfo {
            number: self.pr_number,
            url: self.pr_url.clone(),
            head,
            base,
        }
    }

    pub fn workspace_path(&self) -> Option<PathBuf> {
        self.workspace_path.as_ref().map(PathBuf::from)
    }
}

/// Record for a task that finished without a PR record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ParallelFinishedWithoutPrRecord {
    pub task_id: String,
    #[serde(alias = "worktree_path")]
    pub workspace_path: String,
    pub branch: String,
    pub success: bool,
    pub finished_at: String,
    #[serde(default)]
    pub reason: ParallelNoPrReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ParallelFinishedWithoutPrRecord {
    pub fn new(
        task_id: &str,
        workspace: &WorkspaceSpec,
        success: bool,
        finished_at: String,
        reason: ParallelNoPrReason,
        message: Option<String>,
    ) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_path: workspace.path.to_string_lossy().to_string(),
            branch: workspace.branch.clone(),
            success,
            finished_at,
            reason,
            message,
        }
    }
}

pub(crate) fn state_file_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph/cache/parallel/state.json")
}

pub(crate) fn load_state(path: &Path) -> Result<Option<ParallelStateFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read parallel state {}", path.display()))?;
    let state = crate::jsonc::parse_jsonc::<ParallelStateFile>(&raw, "parallel state")?;
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
/// For each PR record where `merged == false` and `lifecycle == Open`,
/// queries GitHub to determine if the PR is still open. Updates the
/// record's lifecycle and merged flag based on the current state.
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
        // Skip already merged records
        if record.merged || !matches!(record.lifecycle, ParallelPrLifecycle::Open) {
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
                        record.merged = true;
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
        state.upsert_finished_without_pr(ParallelFinishedWithoutPrRecord {
            task_id: "RQ-0009".to_string(),
            workspace_path: "/tmp/workspace/RQ-0009".to_string(),
            branch: "ralph/RQ-0009".to_string(),
            success: true,
            finished_at: "2026-02-01T01:00:00Z".to_string(),
            reason: ParallelNoPrReason::AutoPrDisabled,
            message: Some("auto_pr disabled".to_string()),
        });
        state.upsert_pr(ParallelPrRecord {
            task_id: "RQ-0001".to_string(),
            pr_number: 5,
            pr_url: "https://example.com/pr/5".to_string(),
            head: Some("ralph/RQ-0001".to_string()),
            base: Some("main".to_string()),
            workspace_path: Some("/tmp/workspace".to_string()),
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
        });

        save_state(&path, &state)?;
        let loaded = load_state(&path)?.expect("state");
        assert_eq!(loaded.base_branch, "main");
        assert_eq!(loaded.prs.len(), 1);
        assert_eq!(loaded.finished_without_pr.len(), 1);
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
        assert!(state.finished_without_pr.is_empty());
        Ok(())
    }

    #[test]
    fn state_deserialization_accepts_legacy_worktree_path_in_prs() -> Result<()> {
        let raw = r#"{
            "started_at":"2026-02-01T00:00:00Z",
            "base_branch":"main",
            "merge_method":"squash",
            "merge_when":"as_created",
            "tasks_in_flight":[],
            "prs":[{"task_id":"RQ-0001","pr_number":5,"pr_url":"https://example.com/pr/5","worktree_path":"/tmp/wt","merged":false}]
        }"#;
        let state: ParallelStateFile = serde_json::from_str(raw)?;
        assert_eq!(state.prs.len(), 1);
        assert_eq!(state.prs[0].workspace_path.as_deref(), Some("/tmp/wt"));
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
        assert_eq!(state.finished_without_pr.len(), 1);
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
        assert!(state.finished_without_pr.is_empty());
        Ok(())
    }

    #[test]
    fn finished_without_pr_reason_defaults_to_unknown() {
        let raw = r#"{
            "task_id":"RQ-0010",
            "workspace_path":"/tmp/ws/RQ-0010",
            "branch":"ralph/RQ-0010",
            "success":true,
            "finished_at":"2026-02-01T02:00:00Z"
        }"#;
        let record: ParallelFinishedWithoutPrRecord = serde_json::from_str(raw).unwrap();
        assert!(matches!(record.reason, ParallelNoPrReason::Unknown));
    }

    #[test]
    fn pr_record_uses_fallbacks_when_missing() {
        let record = ParallelPrRecord {
            task_id: "RQ-0002".to_string(),
            pr_number: 9,
            pr_url: "https://example.com/pr/9".to_string(),
            head: None,
            base: None,
            workspace_path: None,
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
        };
        let info = record.pr_info("ralph/RQ-0002", "main");
        assert_eq!(info.head, "ralph/RQ-0002");
        assert_eq!(info.base, "main");
    }

    #[test]
    fn pr_lifecycle_defaults_to_open() {
        // Verify backward compatibility: old state files without lifecycle default to Open
        let raw = r#"{
            "task_id":"RQ-0001",
            "pr_number":5,
            "pr_url":"https://example.com/pr/5",
            "head":"ralph/RQ-0001",
            "base":"main",
            "workspace_path":"/tmp/ws",
            "merged":false
        }"#;
        let record: ParallelPrRecord = serde_json::from_str(raw).unwrap();
        assert!(matches!(record.lifecycle, ParallelPrLifecycle::Open));
        assert!(!record.merged);
    }

    #[test]
    fn pr_lifecycle_round_trips() {
        let record = ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 10,
            pr_url: "https://example.com/pr/10".to_string(),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: true,
            lifecycle: ParallelPrLifecycle::Merged,
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: ParallelPrRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.lifecycle, ParallelPrLifecycle::Merged));
        assert!(parsed.merged);
    }

    // Tests for reconcile_pr_records with stubbed gh binary
    use std::io::Write;
    use std::sync::Mutex;

    // Guard to ensure PATH mutations don't run concurrently
    static PATH_GUARD: Mutex<()> = Mutex::new(());

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
        let _guard = PATH_GUARD.lock().unwrap();
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

        // Prepend fake gh to PATH
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bin_dir.display(), original_path);
        unsafe {
            std::env::set_var("PATH", &new_path);
        }

        let mut state_file = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        // Add 3 PR records, all initially Open and unmerged
        state_file.upsert_pr(ParallelPrRecord {
            task_id: "RQ-0001".to_string(),
            pr_number: 1,
            pr_url: "https://example.com/pr/1".to_string(),
            head: Some("ralph/RQ-0001".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
        });
        state_file.upsert_pr(ParallelPrRecord {
            task_id: "RQ-0002".to_string(),
            pr_number: 2,
            pr_url: "https://example.com/pr/2".to_string(),
            head: Some("ralph/RQ-0002".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
        });
        state_file.upsert_pr(ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 3,
            pr_url: "https://example.com/pr/3".to_string(),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
        });

        let summary = reconcile_pr_records(temp.path(), &mut state_file)?;

        // Restore PATH
        unsafe {
            std::env::set_var("PATH", original_path);
        }

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
        assert!(!pr1.merged);

        assert!(matches!(pr2.lifecycle, ParallelPrLifecycle::Closed));
        assert!(!pr2.merged);

        assert!(matches!(pr3.lifecycle, ParallelPrLifecycle::Merged));
        assert!(pr3.merged);

        Ok(())
    }

    #[test]
    fn reconcile_pr_records_handles_gh_errors_gracefully() -> Result<()> {
        let _guard = PATH_GUARD.lock().unwrap();
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

        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bin_dir.display(), original_path);
        unsafe {
            std::env::set_var("PATH", &new_path);
        }

        let mut state_file = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state_file.upsert_pr(ParallelPrRecord {
            task_id: "RQ-0001".to_string(),
            pr_number: 1,
            pr_url: "https://example.com/pr/1".to_string(),
            head: Some("ralph/RQ-0001".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
        });
        state_file.upsert_pr(ParallelPrRecord {
            task_id: "RQ-0002".to_string(),
            pr_number: 2,
            pr_url: "https://example.com/pr/2".to_string(),
            head: Some("ralph/RQ-0002".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
        });

        let summary = reconcile_pr_records(temp.path(), &mut state_file)?;

        // Restore PATH
        unsafe {
            std::env::set_var("PATH", original_path);
        }

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
        assert!(!pr2.merged);

        Ok(())
    }
}
