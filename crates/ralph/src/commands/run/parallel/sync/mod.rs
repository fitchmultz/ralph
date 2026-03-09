//! State synchronization and git helpers for parallel workers.
//!
//! Responsibilities:
//! - Sync repo-local runtime state into worker workspaces.
//! - Commit changes on worker failure when diagnostics are needed.
//! - Provide push helpers for workspace branch synchronization.
//!
//! Not handled here:
//! - Worker lifecycle (see `super::worker`).
//! - Coordinator orchestration (see `super::orchestration`).
//!
//! Invariants/assumptions:
//! - Worker queue/done paths are seeded from coordinator resolved paths.
//! - Workspace paths are valid and writable.

mod common;
mod gitignored;
mod runtime;

use crate::config;
use crate::git;
use anyhow::{Context, Result};
use std::path::Path;

use gitignored::sync_gitignored;
use runtime::sync_ralph_runtime_tree;

/// Sync ralph state files from repo root to workspace.
///
/// Syncs `.ralph/` runtime files plus gitignored allowlisted files.
/// Ephemeral `.ralph` runtime paths are intentionally NOT synchronized.
/// Queue/done files are seeded explicitly using resolved queue/done paths so
/// parallel workers work with `.jsonc` migrations and gitignored `.ralph` setups.
pub(crate) fn sync_ralph_state(resolved: &config::Resolved, workspace_path: &Path) -> Result<()> {
    let target = workspace_path.join(".ralph");
    std::fs::create_dir_all(&target)
        .with_context(|| format!("create workspace ralph dir {}", target.display()))?;

    let source = resolved.repo_root.join(".ralph");
    sync_ralph_runtime_tree(resolved, &source, &target)?;
    sync_worker_bookkeeping_files(resolved, workspace_path)?;
    sync_gitignored(&resolved.repo_root, workspace_path)?;

    Ok(())
}

/// Commit any pending changes in the workspace after a failure.
/// Returns true if changes were committed, false if there were no changes.
#[allow(dead_code)]
pub(crate) fn commit_failure_changes(workspace_path: &Path, task_id: &str) -> Result<bool> {
    let status = git::status_porcelain(workspace_path)?;
    if status.trim().is_empty() {
        return Ok(false);
    }

    let message = format!("WIP: {} (failed run)", task_id);
    match git::commit_all(workspace_path, &message) {
        Ok(()) => Ok(true),
        Err(err) => match err {
            git::GitError::NoChangesToCommit => Ok(false),
            _ => Err(err.into()),
        },
    }
}

/// Ensure the current branch in the workspace is pushed to upstream.
#[allow(dead_code)]
pub(crate) fn ensure_branch_pushed(workspace_path: &Path) -> Result<()> {
    git::push_upstream_with_rebase(workspace_path)
        .with_context(|| "push branch to upstream (auto-rebase on rejection)")
}

fn sync_worker_bookkeeping_files(resolved: &config::Resolved, workspace_path: &Path) -> Result<()> {
    sync_worker_bookkeeping_file(resolved, workspace_path, &resolved.queue_path, "queue")?;
    sync_worker_bookkeeping_file(resolved, workspace_path, &resolved.done_path, "done")?;
    Ok(())
}

fn sync_worker_bookkeeping_file(
    resolved: &config::Resolved,
    workspace_path: &Path,
    source_path: &Path,
    label: &str,
) -> Result<()> {
    let target_path = super::path_map::map_resolved_path_into_workspace(
        &resolved.repo_root,
        workspace_path,
        source_path,
        label,
    )
    .with_context(|| format!("map {} bookkeeping path into workspace", label))?;

    common::sync_file_if_exists(source_path, &target_path)
        .with_context(|| format!("sync {} bookkeeping file to workspace", label))
}

#[cfg(test)]
mod tests;
