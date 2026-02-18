//! Merge runner for parallel PRs and AI-based conflict resolution.
//!
//! NOTE: This module is deprecated in favor of merge-agent subprocess architecture.
//! It is kept for backward compatibility and tests but should not be used in new code.
//! See `spawn_merge_agent` in `parallel/mod.rs` for the new approach.
//!
//! Responsibilities:
//! - Consume PR work items and attempt merges based on configured policy.
//! - Validate PR head branch names match the expected naming convention.
//! - Resolve merge conflicts using an AI runner when enabled.
//! - Emit merge results for downstream cleanup.
//!
//! Not handled here:
//! - Worker orchestration or task selection (see `parallel/mod.rs`).
//! - PR creation (see `git/pr.rs`).
//! - Blocker persistence (handled by supervisor in `parallel/mod.rs`).
//! - Task finalization (handled by merge-agent subprocess).
//!
//! Invariants/assumptions:
//! - PRs originate from branches named with the configured prefix.
//! - Workspaces remain available until merge completion or failure.
//! - Each work item carries a trusted task_id (from queue/state, not derived from PR head).
//! - Task finalization is handled by merge-agent, not this module.

#![allow(dead_code)]

mod conflict;
mod git_ops;
mod validation;

#[cfg(test)]
mod tests;

use crate::commands::run::parallel::merge_runner::conflict::resolve_conflicts;
use crate::commands::run::parallel::merge_runner::validation::validate_pr_head;
use crate::config;
use crate::contracts::{ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod};
use crate::git;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

// Re-export items needed by tests and submodules

/// RAII guard that ensures cleanup of the .base-sync workspace on any exit path.
///
/// This guard performs best-effort cleanup of the ephemeral .base-sync directory
/// when dropped. It validates the path before deletion to prevent accidental
/// removal of unexpected directories.
pub(crate) struct BaseSyncWorkspaceCleanupGuard {
    workspace_root: PathBuf,
    base_sync_path: PathBuf,
}

impl BaseSyncWorkspaceCleanupGuard {
    pub(crate) fn new(workspace_root: &Path, base_sync_path: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            base_sync_path: base_sync_path.to_path_buf(),
        }
    }
}

impl Drop for BaseSyncWorkspaceCleanupGuard {
    fn drop(&mut self) {
        // Defense-in-depth: only delete a directory literally named ".base-sync"
        // that lives under the configured workspace root.
        let is_base_sync = self
            .base_sync_path
            .file_name()
            .is_some_and(|n| n == std::ffi::OsStr::new(".base-sync"));
        if !is_base_sync || !self.base_sync_path.starts_with(&self.workspace_root) {
            log::warn!(
                "Refusing to remove unexpected base-sync path: {} (workspace_root={})",
                self.base_sync_path.display(),
                self.workspace_root.display()
            );
            return;
        }

        match std::fs::remove_dir_all(&self.base_sync_path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => log::warn!(
                "Failed to remove base-sync workspace {}: {}",
                self.base_sync_path.display(),
                err
            ),
        }
    }
}

/// Work item for the merge runner containing trusted task_id, PR info, and workspace metadata.
#[derive(Debug, Clone)]
pub(crate) struct MergeWorkItem {
    pub task_id: String,
    pub pr: git::PrInfo,
    /// Optional path to the worker workspace for validation and merge operations.
    pub workspace_path: Option<PathBuf>,
}

pub(crate) enum MergeQueueSource {
    AsCreated(mpsc::Receiver<MergeWorkItem>),
    AfterAll(Vec<MergeWorkItem>),
}

#[derive(Debug, Clone)]
pub(crate) struct MergeResult {
    pub task_id: String,
    pub merged: bool,
    /// Human-readable reason this PR should be skipped by the supervisor.
    /// When Some(_), merged must be false and queue/done/productivity bytes must be None.
    pub merge_blocker: Option<String>,
    pub queue_bytes: Option<Vec<u8>>,
    pub done_bytes: Option<Vec<u8>>,
    pub productivity_bytes: Option<Vec<u8>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_merge_runner(
    resolved: &config::Resolved,
    merge_method: ParallelMergeMethod,
    conflict_policy: ConflictPolicy,
    merge_runner: MergeRunnerConfig,
    retries: u8,
    pr_queue: MergeQueueSource,
    workspace_root: &Path,
    delete_branch: bool,
    merge_result_tx: mpsc::Sender<MergeResult>,
    merge_stop: Arc<AtomicBool>,
) -> Result<()> {
    let handle_one = |work_item: MergeWorkItem| -> Result<()> {
        if merge_stop.load(Ordering::SeqCst) {
            return Ok(());
        }

        if let Some(result) = handle_work_item(
            resolved,
            work_item,
            merge_method,
            conflict_policy,
            merge_runner.clone(),
            retries,
            workspace_root,
            delete_branch,
            &merge_stop,
        )? {
            let _ = merge_result_tx.send(result);
        }

        Ok(())
    };

    match pr_queue {
        MergeQueueSource::AsCreated(rx) => {
            for work_item in rx.iter() {
                handle_one(work_item)?;
            }
        }
        MergeQueueSource::AfterAll(work_items) => {
            for work_item in work_items {
                handle_one(work_item)?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_work_item(
    resolved: &config::Resolved,
    work_item: MergeWorkItem,
    merge_method: ParallelMergeMethod,
    conflict_policy: ConflictPolicy,
    merge_runner: MergeRunnerConfig,
    retries: u8,
    workspace_root: &Path,
    delete_branch: bool,
    merge_stop: &AtomicBool,
) -> Result<Option<MergeResult>> {
    if merge_stop.load(Ordering::SeqCst) {
        return Ok(None);
    }

    let branch_prefix = resolved
        .config
        .parallel
        .branch_prefix
        .clone()
        .unwrap_or_else(|| "ralph/".to_string());

    // Validate the PR head matches expected naming convention
    if let Err(reason) = validate_pr_head(&branch_prefix, &work_item.task_id, &work_item.pr.head) {
        log::warn!(
            "Skipping PR {} due to head mismatch for task {}: {}. \
             This usually means the branch_prefix config changed or the PR head was renamed.",
            work_item.pr.number,
            work_item.task_id,
            reason
        );
        return Ok(Some(MergeResult {
            task_id: work_item.task_id,
            merged: false,
            merge_blocker: Some(reason),
            queue_bytes: None,
            done_bytes: None,
            productivity_bytes: None,
        }));
    }

    let merged = merge_pr_with_retries(
        resolved,
        &work_item.pr,
        merge_method,
        conflict_policy,
        merge_runner,
        retries,
        workspace_root,
        &work_item.task_id,
        delete_branch,
        merge_stop,
    )?;

    if merged {
        // Note: Task finalization is now handled by the merge-agent subprocess.
        // This deprecated merge-runner module no longer performs queue/done sync.
        // The merge-agent is responsible for calling queue::complete_task directly.
        Ok(Some(MergeResult {
            task_id: work_item.task_id,
            merged: true,
            merge_blocker: None,
            queue_bytes: None,
            done_bytes: None,
            productivity_bytes: None,
        }))
    } else {
        Ok(None)
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_pr_with_retries(
    resolved: &config::Resolved,
    pr: &git::PrInfo,
    merge_method: ParallelMergeMethod,
    conflict_policy: ConflictPolicy,
    merge_runner: MergeRunnerConfig,
    retries: u8,
    workspace_root: &Path,
    task_id: &str,
    delete_branch: bool,
    merge_stop: &AtomicBool,
) -> Result<bool> {
    let mut attempts = 0u8;
    loop {
        if merge_stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        attempts += 1;
        let status = git::pr_merge_status(&resolved.repo_root, pr.number)?;
        if status.is_draft {
            log::info!("Skipping draft PR {} (not eligible for merge).", pr.number);
            return Ok(false);
        }
        match status.merge_state {
            git::MergeState::Clean => {
                if let Err(err) =
                    git::merge_pr(&resolved.repo_root, pr.number, merge_method, delete_branch)
                {
                    log::warn!("Merge failed for PR {}: {:#}", pr.number, err);
                    return Ok(false);
                }
                return Ok(true);
            }
            git::MergeState::Dirty => match conflict_policy {
                ConflictPolicy::AutoResolve => {
                    resolve_conflicts(resolved, pr, workspace_root, task_id, &merge_runner)?;
                }
                ConflictPolicy::RetryLater => {
                    if attempts >= retries {
                        return Ok(false);
                    }
                    sleep_backoff(attempts);
                    continue;
                }
                ConflictPolicy::Reject => return Ok(false),
            },
            git::MergeState::Other(status) => {
                log::info!(
                    "PR {} merge state is {}; retrying ({}/{})",
                    pr.number,
                    status,
                    attempts,
                    retries
                );
                if attempts >= retries {
                    return Ok(false);
                }
                sleep_backoff(attempts);
            }
        }

        if attempts >= retries {
            return Ok(false);
        }
    }
}

fn sleep_backoff(attempt: u8) {
    let ms = 500_u64.saturating_mul(attempt as u64);
    thread::sleep(Duration::from_millis(ms));
}
