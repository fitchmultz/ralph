//! Git LFS detection helpers.
//!
//! Purpose:
//! - Git LFS detection helpers.
//!
//! Responsibilities:
//! - Detect whether a repository is configured for Git LFS.
//! - List LFS-tracked files via `git lfs ls-files`.
//!
//! Not handled here:
//! - Filter validation or pointer parsing.
//! - Aggregate health reporting.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Known "LFS not installed" failures return empty results instead of errors.

use crate::git::error::{GitError, git_output};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn has_lfs(repo_root: &Path) -> Result<bool> {
    let git_lfs_dir = repo_root.join(".git/lfs");
    if git_lfs_dir.is_dir() {
        return Ok(true);
    }

    let gitattributes = repo_root.join(".gitattributes");
    if gitattributes.is_file() {
        let content = fs::read_to_string(&gitattributes)
            .with_context(|| format!("read .gitattributes in {}", repo_root.display()))?;
        return Ok(content.contains("filter=lfs"));
    }

    Ok(false)
}

pub fn list_lfs_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = git_output(repo_root, &["lfs", "ls-files"])
        .with_context(|| format!("run git lfs ls-files in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git lfs repository")
            || stderr.contains("git: lfs is not a git command")
        {
            return Ok(Vec::new());
        }
        return Err(GitError::CommandFailed {
            args: "lfs ls-files".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        }
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();
    for line in stdout.lines() {
        if let Some((_, path)) = line.rsplit_once(" * ") {
            files.push(path.to_string());
        }
    }
    Ok(files)
}
