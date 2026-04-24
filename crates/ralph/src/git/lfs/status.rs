//! Git LFS status parsing.
//!
//! Purpose:
//! - Git LFS status parsing.
//!
//! Responsibilities:
//! - Run `git lfs status` and parse staged/unstaged LFS health data.
//!
//! Not handled here:
//! - Filter validation or pointer file inspection.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Known "LFS unavailable" failures return an empty summary.

use super::types::LfsStatusSummary;
use crate::git::error::{GitError, git_output};
use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn check_lfs_status(repo_root: &Path) -> Result<LfsStatusSummary, GitError> {
    let output = git_output(repo_root, &["lfs", "status"])
        .with_context(|| format!("run git lfs status in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git lfs repository")
            || stderr.contains("git: lfs is not a git command")
        {
            return Ok(LfsStatusSummary::default());
        }
        return Err(GitError::CommandFailed {
            args: "lfs status".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut summary = LfsStatusSummary::default();
    let mut in_staged_section = false;
    let mut in_unstaged_section = false;

    for line in stdout.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Objects to be committed:") {
            in_staged_section = true;
            in_unstaged_section = false;
            continue;
        }
        if trimmed.starts_with("Objects not staged for commit:") {
            in_staged_section = false;
            in_unstaged_section = true;
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('(') {
            continue;
        }

        if let Some((file_path, status)) = trimmed.split_once(" (") {
            let file_path = file_path.trim();
            let status = status.trim_end_matches(')');

            if in_staged_section {
                if status.starts_with("LFS:") {
                    summary.staged_lfs.push(file_path.to_string());
                } else if status.starts_with("Git:") {
                    summary.staged_not_lfs.push(file_path.to_string());
                }
            } else if in_unstaged_section && status.starts_with("LFS:") {
                summary.unstaged_lfs.push(file_path.to_string());
            }
        }
    }

    Ok(summary)
}
