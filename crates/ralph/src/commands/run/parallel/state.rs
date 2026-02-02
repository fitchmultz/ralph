//! Parallel run state persistence for crash recovery.
//!
//! Responsibilities:
//! - Define the parallel state file format and helpers.
//! - Persist and reload state for in-flight tasks and PRs.
//!
//! Not handled here:
//! - Worker orchestration or process management (see `parallel/mod.rs`).
//! - PR merge logic (see `merge_runner`).
//!
//! Invariants/assumptions:
//! - State file lives at `.ralph/cache/parallel/state.json`.
//! - Callers update and persist state after each significant transition.

use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};
use crate::fsutil;
use crate::git::WorktreeSpec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParallelStateFile {
    pub started_at: String,
    pub base_branch: String,
    pub merge_method: ParallelMergeMethod,
    pub merge_when: ParallelMergeWhen,
    pub tasks_in_flight: Vec<ParallelTaskRecord>,
    pub prs: Vec<ParallelPrRecord>,
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
            existing.merged = true;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParallelTaskRecord {
    pub task_id: String,
    pub workspace_path: String,
    pub branch: String,
    pub pid: Option<u32>,
}

impl ParallelTaskRecord {
    pub fn new(task_id: &str, worktree: &WorktreeSpec, pid: u32) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_path: worktree.path.to_string_lossy().to_string(),
            branch: worktree.branch.clone(),
            pid: Some(pid),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParallelPrRecord {
    pub task_id: String,
    pub pr_number: u32,
    pub pr_url: String,
    #[serde(default)]
    pub head: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub workspace_path: Option<String>,
    pub merged: bool,
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
        state.upsert_pr(ParallelPrRecord {
            task_id: "RQ-0001".to_string(),
            pr_number: 5,
            pr_url: "https://example.com/pr/5".to_string(),
            head: Some("ralph/RQ-0001".to_string()),
            base: Some("main".to_string()),
            workspace_path: Some("/tmp/workspace".to_string()),
            merged: false,
        });

        save_state(&path, &state)?;
        let loaded = load_state(&path)?.expect("state");
        assert_eq!(loaded.base_branch, "main");
        assert_eq!(loaded.prs.len(), 1);
        Ok(())
    }

    #[test]
    fn state_deserialization_rejects_legacy_worktree_path_in_tasks() {
        let raw = r#"{
            "started_at":"2026-02-01T00:00:00Z",
            "base_branch":"main",
            "merge_method":"squash",
            "merge_when":"as_created",
            "tasks_in_flight":[{"task_id":"RQ-0001","worktree_path":"/tmp/wt","branch":"b","pid":1}],
            "prs":[]
        }"#;
        let err = serde_json::from_str::<ParallelStateFile>(raw).unwrap_err();
        assert!(err.to_string().contains("worktree_path"));
    }

    #[test]
    fn state_deserialization_rejects_legacy_worktree_path_in_prs() {
        let raw = r#"{
            "started_at":"2026-02-01T00:00:00Z",
            "base_branch":"main",
            "merge_method":"squash",
            "merge_when":"as_created",
            "tasks_in_flight":[],
            "prs":[{"task_id":"RQ-0001","pr_number":5,"pr_url":"https://example.com/pr/5","worktree_path":"/tmp/wt","merged":false}]
        }"#;
        let err = serde_json::from_str::<ParallelStateFile>(raw).unwrap_err();
        assert!(err.to_string().contains("worktree_path"));
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
        };
        let info = record.pr_info("ralph/RQ-0002", "main");
        assert_eq!(info.head, "ralph/RQ-0002");
        assert_eq!(info.base, "main");
    }
}
