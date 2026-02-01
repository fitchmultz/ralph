//! Git commit and push operations.
//!
//! This module provides functions for creating commits and pushing to upstream
//! remotes. It handles error classification and provides clear feedback on failures.
//!
//! # Invariants
//! - Upstream must be configured before pushing
//! - Empty commit messages are rejected
//! - No-changes commits are rejected
//!
//! # What this does NOT handle
//! - Status checking (see git/status.rs)
//! - LFS validation (see git/lfs.rs)
//! - Repository cleanliness (see git/clean.rs)

use crate::git::error::{GitError, classify_push_error, git_base_command, git_run};
use anyhow::Context;
use std::path::Path;

use crate::git::status::status_porcelain;

/// Revert uncommitted changes, restoring the working tree to current HEAD.
///
/// This discards ONLY uncommitted changes. It does NOT reset to a pre-run SHA.
pub fn revert_uncommitted(repo_root: &Path) -> Result<(), GitError> {
    // Revert tracked changes in both index and working tree.
    // Prefer `git restore` (modern); fall back to older `git checkout` syntax.
    if git_run(repo_root, &["restore", "--staged", "--worktree", "."]).is_err() {
        // Older git fallback.
        git_run(repo_root, &["checkout", "--", "."]).context("fallback git checkout -- .")?;
        // Ensure staged changes are cleared too.
        git_run(repo_root, &["reset", "--quiet", "HEAD"]).context("git reset --quiet HEAD")?;
    }

    // Remove untracked files/directories created during the run.
    git_run(
        repo_root,
        &[
            "clean",
            "-fd",
            "-e",
            ".env",
            "-e",
            ".env.*",
            "-e",
            ".ralph/cache/completions",
        ],
    )
    .context("git clean -fd -e .env*")?;
    Ok(())
}

/// Create a commit with all changes.
///
/// Stages everything and creates a single commit with the given message.
/// Returns an error if the message is empty or there are no changes to commit.
pub fn commit_all(repo_root: &Path, message: &str) -> Result<(), GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::EmptyCommitMessage);
    }

    git_run(repo_root, &["add", "-A"]).context("git add -A")?;
    let status = status_porcelain(repo_root)?;
    if status.trim().is_empty() {
        return Err(GitError::NoChangesToCommit);
    }

    git_run(repo_root, &["commit", "-m", message]).context("git commit")?;
    Ok(())
}

/// Get the configured upstream for the current branch.
///
/// Returns the upstream reference (e.g. "origin/main") or an error if not configured.
pub fn upstream_ref(repo_root: &Path) -> Result<String, GitError> {
    let output = git_base_command(repo_root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("--symbolic-full-name")
        .arg("@{u}")
        .output()
        .with_context(|| {
            format!(
                "run git rev-parse --abbrev-ref --symbolic-full-name @{{u}} in {}",
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(classify_push_error(&stderr));
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        return Err(GitError::NoUpstreamConfigured);
    }
    Ok(value)
}

/// Check if HEAD is ahead of the configured upstream.
///
/// Returns true if there are local commits that haven't been pushed.
pub fn is_ahead_of_upstream(repo_root: &Path) -> Result<bool, GitError> {
    let upstream = upstream_ref(repo_root)?;
    let range = format!("{upstream}...HEAD");
    let output = git_base_command(repo_root)
        .arg("rev-list")
        .arg("--left-right")
        .arg("--count")
        .arg(range)
        .output()
        .with_context(|| {
            format!(
                "run git rev-list --left-right --count in {}",
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(GitError::CommandFailed {
            args: "rev-list --left-right --count".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    let counts = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = counts.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(GitError::UnexpectedRevListOutput(counts.trim().to_string()));
    }

    let ahead: u32 = parts[1].parse().context("parse ahead count")?;
    Ok(ahead > 0)
}

/// Push HEAD to the configured upstream.
///
/// Returns an error if push fails due to authentication, missing upstream,
/// or other git errors.
pub fn push_upstream(repo_root: &Path) -> Result<(), GitError> {
    let output = git_base_command(repo_root)
        .arg("push")
        .output()
        .with_context(|| format!("run git push in {}", repo_root.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(classify_push_error(&stderr))
}
