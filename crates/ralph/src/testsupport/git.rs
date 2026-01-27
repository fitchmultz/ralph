//! Git helpers for unit tests.
//!
//! Responsibilities:
//! - Initialize temporary git repositories for tests.
//! - Run git commands and capture output in a consistent, checked manner.
//!
//! Not handled:
//! - Real remotes, LFS configuration, or complex git workflows.
//! - Non-test safety checks or user interaction.
//!
//! Invariants/assumptions:
//! - `git` is available on PATH.
//! - Callers provide a writable directory (typically a temp dir).

use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub(crate) fn git_run(repo_root: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()?;
    anyhow::ensure!(status.success(), "git {:?} failed", args);
    Ok(())
}

pub(crate) fn git_output(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()?;
    anyhow::ensure!(output.status.success(), "git {:?} failed", args);
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn init_repo(repo_root: &Path) -> Result<()> {
    git_run(repo_root, &["init"])?;
    git_run(repo_root, &["config", "user.email", "test@example.com"])?;
    git_run(repo_root, &["config", "user.name", "Test User"])?;
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    Ok(())
}

pub(crate) fn commit_all(repo_root: &Path, message: &str) -> Result<()> {
    git_run(repo_root, &["add", "-A"])?;
    git_run(repo_root, &["commit", "-m", message])?;
    Ok(())
}
