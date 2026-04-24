//! Git helpers for unit tests.
//!
//! Purpose:
//! - Git helpers for unit tests.
//!
//! Responsibilities:
//! - Initialize temporary git repositories for tests.
//! - Run git commands and capture output in a consistent, checked manner.
//! - Support bare remotes and multi-repo workflows for testing merge scenarios.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled:
//! - Real remotes over network, LFS configuration, or complex git workflows.
//! - Non-test safety checks or user interaction.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `git` is available on PATH.
//! - Callers provide a writable directory (typically a temp dir).

use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub(crate) fn git_run(repo_root: &Path, args: &[&str]) -> Result<()> {
    let _path_guard = crate::testsupport::path::path_lock()
        .lock()
        .expect("path lock");
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()?;
    anyhow::ensure!(status.success(), "git {:?} failed", args);
    Ok(())
}

pub(crate) fn git_output(repo_root: &Path, args: &[&str]) -> Result<String> {
    let _path_guard = crate::testsupport::path::path_lock()
        .lock()
        .expect("path lock");
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
    git_run(repo_root, &["config", "core.excludesFile", "/dev/null"])?;
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    Ok(())
}

pub(crate) fn commit_all(repo_root: &Path, message: &str) -> Result<()> {
    git_run(repo_root, &["add", "-A"])?;
    if repo_root.join(".ralph").exists() {
        git_run(repo_root, &["add", "-f", ".ralph"])?;
    }
    git_run(repo_root, &["commit", "-m", message])?;
    Ok(())
}

/// Initialize a bare repository suitable for use as a remote.
pub(crate) fn init_bare_repo(repo_root: &Path) -> Result<()> {
    git_run(repo_root, &["init", "--bare"])
}

/// Clone a repository from source to destination.
pub(crate) fn clone_repo(source: &Path, dest: &Path) -> Result<()> {
    let _path_guard = crate::testsupport::path::path_lock()
        .lock()
        .expect("path lock");
    let status = Command::new("git")
        .arg("clone")
        .arg(source)
        .arg(dest)
        .status()?;
    anyhow::ensure!(status.success(), "git clone failed");
    Ok(())
}

/// Configure git user identity in a repository.
pub(crate) fn configure_user(repo_root: &Path) -> Result<()> {
    git_run(repo_root, &["config", "user.email", "test@example.com"])?;
    git_run(repo_root, &["config", "user.name", "Test User"])?;
    Ok(())
}

/// Add a remote to a repository.
pub(crate) fn add_remote(repo_root: &Path, name: &str, url: &Path) -> Result<()> {
    git_run(repo_root, &["remote", "add", name, url.to_str().unwrap()])
}

/// Push a branch to origin.
pub(crate) fn push_branch(repo_root: &Path, branch: &str) -> Result<()> {
    git_run(repo_root, &["push", "origin", branch])
}
