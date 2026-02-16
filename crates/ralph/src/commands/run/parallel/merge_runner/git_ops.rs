//! Git operations for merge runner.
//!
//! Responsibilities:
//! - Low-level git command execution for merge operations.
//! - Status checking and branch pushing.
//!
//! Not handled here:
//! - High-level merge orchestration (see `mod.rs`).
//! - Conflict resolution logic (see `conflict.rs`).

use anyhow::{Context, Result, bail};
use std::path::Path;

/// Run git status --porcelain and return output.
pub(crate) fn git_status(repo_root: &Path) -> Result<String> {
    git_output(repo_root, &["status", "--porcelain"])
}

/// Push branch to upstream with auto-rebase on rejection.
pub(crate) fn push_branch(repo_root: &Path) -> Result<()> {
    crate::git::push_upstream_with_rebase(repo_root)
        .context("push branch to upstream (auto-rebase on rejection)")
}

/// Run a git command, failing on non-zero exit.
pub(crate) fn git_run(repo_root: &Path, args: &[&str]) -> Result<()> {
    let output = crate::git::error::git_base_command(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(())
}

/// Run a git command and return stdout as string, failing on non-zero exit.
pub(crate) fn git_output(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = crate::git::error::git_base_command(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
