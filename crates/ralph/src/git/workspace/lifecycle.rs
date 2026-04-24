//! Workspace lifecycle orchestration.
//!
//! Purpose:
//! - Workspace lifecycle orchestration.
//!
//! Responsibilities:
//! - Create, reuse, reset, and remove isolated git workspaces.
//! - Validate user inputs and filesystem safety around workspace removal.
//! - Bridge path policy and git subprocess helpers into the public workspace API.
//!
//! Not handled here:
//! - Low-level git command execution details.
//! - Workspace path derivation rules.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Task IDs and base branches must be non-empty after trimming.
//! - Forced removal bypasses dirty-worktree checks.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::git_ops::{
    ensure_clean_workspace, ensure_workspace_repo, origin_urls, reset_workspace_to_branch,
    reset_workspace_to_remote_branch,
};

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceSpec {
    pub path: PathBuf,
    #[allow(dead_code)]
    pub branch: String,
}

pub(crate) fn create_workspace_at(
    repo_root: &Path,
    workspace_root: &Path,
    task_id: &str,
    base_branch: &str,
) -> Result<WorkspaceSpec> {
    let trimmed_id = require_trimmed_value(task_id, "workspace task_id")?;
    let branch = require_trimmed_value(base_branch, "workspace base_branch")?;
    let path = workspace_root.join(trimmed_id);

    fs::create_dir_all(workspace_root).with_context(|| {
        format!(
            "create workspace root directory {}",
            workspace_root.display()
        )
    })?;

    ensure_workspace_repo(repo_root, &path)?;
    let (fetch_url, push_url) = origin_urls(repo_root)?;
    reset_workspace_to_branch(&path, &branch, &fetch_url, &push_url)?;

    Ok(WorkspaceSpec { path, branch })
}

/// Ensures a workspace exists and is properly configured for the given branch.
///
/// Note: Kept for legacy callers that need a branch-specific workspace reset helper.
#[allow(dead_code)]
pub(crate) fn ensure_workspace_exists(
    repo_root: &Path,
    workspace_path: &Path,
    branch: &str,
) -> Result<()> {
    let branch = require_trimmed_value(branch, "workspace branch")?;
    ensure_workspace_repo(repo_root, workspace_path)?;

    let (fetch_url, push_url) = origin_urls(repo_root)?;
    reset_workspace_to_remote_branch(workspace_path, &branch, &fetch_url, &push_url)
}

pub(crate) fn remove_workspace(
    workspace_root: &Path,
    spec: &WorkspaceSpec,
    force: bool,
) -> Result<()> {
    if !spec.path.exists() {
        return Ok(());
    }
    if !spec.path.starts_with(workspace_root) {
        bail!(
            "workspace path {} is outside root {}",
            spec.path.display(),
            workspace_root.display()
        );
    }
    if force {
        fs::remove_dir_all(&spec.path)
            .with_context(|| format!("remove workspace {}", spec.path.display()))?;
        return Ok(());
    }

    ensure_clean_workspace(&spec.path)?;
    fs::remove_dir_all(&spec.path)
        .with_context(|| format!("remove workspace {}", spec.path.display()))
}

fn require_trimmed_value(value: &str, label: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{label} must be non-empty");
    }
    Ok(trimmed.to_string())
}
