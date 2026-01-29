//! Git-related error types and error classification.
//!
//! This module defines all error types that can occur during git operations.
//! It provides structured error variants for common failure modes like dirty
//! repositories, authentication failures, and missing upstream configuration.
//!
//! # Invariants
//! - All error types implement `Send + Sync` for anyhow compatibility
//! - Error messages should be actionable and include context where possible
//!
//! # What this does NOT handle
//! - Success cases or happy-path results
//! - Non-git related errors (use anyhow for those)

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use thiserror::Error;

/// Errors that can occur during git operations.
#[derive(Error, Debug)]
pub enum GitError {
    #[error("repo is dirty; commit/stash your changes before running Ralph.{details}")]
    DirtyRepo { details: String },

    #[error("git {args} failed (code={code:?}): {stderr}")]
    CommandFailed {
        args: String,
        code: Option<i32>,
        stderr: String,
    },

    #[error("git push failed: no upstream configured for current branch. Set it with: git push -u origin <branch> OR git branch --set-upstream-to origin/<branch>.")]
    NoUpstream,

    #[error("git push failed: authentication/permission denied. Verify the remote URL, credentials, and that you have push access.")]
    AuthFailed,

    #[error("git push failed: {0}")]
    PushFailed(String),

    #[error("commit message is empty")]
    EmptyCommitMessage,

    #[error("no changes to commit")]
    NoChangesToCommit,

    #[error("no upstream configured for current branch")]
    NoUpstreamConfigured,

    #[error("unexpected rev-list output: {0}")]
    UnexpectedRevListOutput(String),

    #[error("Git LFS filter misconfigured: {details}")]
    LfsFilterMisconfigured { details: String },

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Classify a push error from stderr into a specific GitError variant.
pub fn classify_push_error(stderr: &str) -> GitError {
    let raw = stderr.trim();
    let lower = raw.to_lowercase();

    if lower.contains("no upstream")
        || lower.contains("set-upstream")
        || lower.contains("set the remote as upstream")
    {
        return GitError::NoUpstream;
    }

    if lower.contains("permission denied")
        || lower.contains("authentication failed")
        || lower.contains("access denied")
        || lower.contains("could not read from remote repository")
        || lower.contains("repository not found")
    {
        return GitError::AuthFailed;
    }

    let detail = if raw.is_empty() {
        "unknown git error".to_string()
    } else {
        raw.to_string()
    };
    GitError::PushFailed(detail)
}

/// Build a base git command with fsmonitor disabled.
///
/// Some environments (notably when fsmonitor is enabled but unhealthy) emit:
///   error: fsmonitor_ipc__send_query: ... '.git/fsmonitor--daemon.ipc'
/// This is noisy and can confuse agents/automation. Disabling fsmonitor for
/// Ralph's git invocations avoids that class of failures.
pub fn git_base_command(repo_root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-c").arg("core.fsmonitor=false");
    cmd.arg("-C").arg(repo_root);
    cmd
}

/// Run a git command and return an error on failure.
pub fn git_run(repo_root: &Path, args: &[&str]) -> Result<(), GitError> {
    let output = git_base_command(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Err(GitError::CommandFailed {
        args: args.join(" "),
        code: output.status.code(),
        stderr: stderr.trim().to_string(),
    })
}
