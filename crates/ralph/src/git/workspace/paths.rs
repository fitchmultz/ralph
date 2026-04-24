//! Workspace path resolution helpers.
//!
//! Purpose:
//! - Workspace path resolution helpers.
//!
//! Responsibilities:
//! - Compute the effective workspace root from config and repository location.
//! - Keep tilde expansion and relative-path policy centralized.
//! - Provide the default workspace-root convention for parallel runs.
//!
//! Not handled here:
//! - Workspace creation, cleanup, or git subprocess execution.
//! - Validation of workspace contents.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Relative workspace roots are resolved against `repo_root`.
//! - The default workspace root stays outside the repository working tree.

use std::path::{Path, PathBuf};

use crate::contracts::Config;
use crate::fsutil;

pub(crate) fn workspace_root(repo_root: &Path, cfg: &Config) -> PathBuf {
    let raw = cfg
        .parallel
        .workspace_root
        .clone()
        .unwrap_or_else(|| default_workspace_root(repo_root));

    let root = fsutil::expand_tilde(&raw);
    if root.is_absolute() {
        root
    } else {
        repo_root.join(root)
    }
}

fn default_workspace_root(repo_root: &Path) -> PathBuf {
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repo");
    let parent = repo_root.parent().unwrap_or(repo_root);
    parent.join(".workspaces").join(repo_name).join("parallel")
}
