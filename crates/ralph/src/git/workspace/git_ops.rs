//! Git subprocess helpers for workspace setup.
//!
//! Purpose:
//! - Git subprocess helpers for workspace setup.
//!
//! Responsibilities:
//! - Resolve fetch/push origin URLs from the source repository.
//! - Clone, retarget, fetch, checkout, reset, and inspect git state for workspaces.
//! - Keep git command error handling and stderr reporting consistent.
//!
//! Not handled here:
//! - Workspace path policy.
//! - High-level workspace lifecycle orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `git_output` is the canonical subprocess surface for these operations.
//! - A missing `origin` remote is a hard error for parallel workspace setup.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::git::error::git_output;

pub(crate) fn origin_urls(repo_root: &Path) -> Result<(String, String)> {
    let fetch = remote_url(repo_root, &["remote", "get-url", "origin"])?;
    let push = remote_url(repo_root, &["remote", "get-url", "--push", "origin"])?;

    match (fetch, push) {
        (Some(fetch_url), Some(push_url)) => Ok((fetch_url, push_url)),
        (Some(fetch_url), None) => Ok((fetch_url.clone(), fetch_url)),
        (None, Some(push_url)) => Ok((push_url.clone(), push_url)),
        (None, None) => {
            bail!(
                "No 'origin' git remote configured (required for parallel mode).\n\
Parallel workspaces need a pushable `origin` remote to retarget and push branches.\n\
\n\
Fix options:\n\
1) Add origin:\n\
   git remote add origin <url>\n\
2) Or disable parallel mode:\n\
   run without `--parallel` (use the non-parallel run loop)\n"
            )
        }
    }
}

pub(super) fn ensure_workspace_repo(repo_root: &Path, workspace_path: &Path) -> Result<()> {
    if workspace_path.exists() {
        if !workspace_path.join(".git").exists() {
            std::fs::remove_dir_all(workspace_path).with_context(|| {
                format!(
                    "remove invalid workspace (missing .git) {}",
                    workspace_path.display()
                )
            })?;
            clone_repo_from_local(repo_root, workspace_path)?;
        }
    } else {
        if let Some(parent) = workspace_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("create workspace parent directory {}", parent.display())
            })?;
        }
        clone_repo_from_local(repo_root, workspace_path)?;
    }

    Ok(())
}

pub(super) fn reset_workspace_to_branch(
    workspace_path: &Path,
    branch: &str,
    fetch_url: &str,
    push_url: &str,
) -> Result<()> {
    retarget_origin(workspace_path, fetch_url, push_url)?;
    log_best_effort_fetch(workspace_path);

    let base_ref = resolve_base_ref(workspace_path, branch)?;
    checkout_branch_from_base(workspace_path, branch, &base_ref)?;
    hard_reset_and_clean(workspace_path, &base_ref)
}

pub(super) fn reset_workspace_to_remote_branch(
    workspace_path: &Path,
    branch: &str,
    fetch_url: &str,
    push_url: &str,
) -> Result<()> {
    retarget_origin(workspace_path, fetch_url, push_url)?;
    log_best_effort_fetch(workspace_path);

    let remote_ref = format!("origin/{branch}");
    checkout_branch_from_base(workspace_path, branch, &remote_ref)?;
    hard_reset_and_clean(workspace_path, &remote_ref)
}

pub(super) fn ensure_clean_workspace(workspace_path: &Path) -> Result<()> {
    let output = git_output(workspace_path, &["status", "--porcelain"])
        .with_context(|| format!("run git status in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {}", stderr.trim());
    }
    let status = String::from_utf8_lossy(&output.stdout);
    if !status.trim().is_empty() {
        bail!("workspace is dirty; use force to remove");
    }
    Ok(())
}

fn clone_repo_from_local(repo_root: &Path, dest: &Path) -> Result<()> {
    let dest_owned = dest.to_string_lossy().into_owned();
    let output = git_output(repo_root, &["clone", "--no-hardlinks", ".", &dest_owned])
        .with_context(|| format!("run git clone into {}", dest.display()))?;
    ensure_git_success(output.status.success(), "git clone failed", &output.stderr)
}

fn remote_url(repo_root: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = git_output(repo_root, args)
        .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn retarget_origin(workspace_path: &Path, fetch_url: &str, push_url: &str) -> Result<()> {
    let fetch_output = git_output(
        workspace_path,
        &["remote", "set-url", "origin", fetch_url.trim()],
    )
    .with_context(|| format!("set origin fetch url in {}", workspace_path.display()))?;
    ensure_git_success(
        fetch_output.status.success(),
        "git remote set-url origin failed",
        &fetch_output.stderr,
    )?;

    let push_output = git_output(
        workspace_path,
        &["remote", "set-url", "--push", "origin", push_url.trim()],
    )
    .with_context(|| format!("set origin push url in {}", workspace_path.display()))?;
    ensure_git_success(
        push_output.status.success(),
        "git remote set-url --push origin failed",
        &push_output.stderr,
    )
}

fn log_best_effort_fetch(workspace_path: &Path) {
    if let Err(err) = fetch_origin(workspace_path) {
        log::debug!("Best-effort git fetch failed (expected in tests/offline): {err}");
    }
}

fn fetch_origin(workspace_path: &Path) -> Result<()> {
    let output = git_output(workspace_path, &["fetch", "origin", "--prune"])
        .with_context(|| format!("run git fetch in {}", workspace_path.display()))?;
    ensure_git_success(output.status.success(), "git fetch failed", &output.stderr)
}

fn resolve_base_ref(workspace_path: &Path, base_branch: &str) -> Result<String> {
    let remote_ref = format!("refs/remotes/origin/{base_branch}");
    if git_ref_exists(workspace_path, &remote_ref)? {
        return Ok(format!("origin/{base_branch}"));
    }
    let local_ref = format!("refs/heads/{base_branch}");
    if git_ref_exists(workspace_path, &local_ref)? {
        return Ok(base_branch.to_string());
    }
    bail!("base branch '{base_branch}' not found in workspace");
}

fn git_ref_exists(repo_root: &Path, full_ref: &str) -> Result<bool> {
    let output = git_output(repo_root, &["show-ref", "--verify", "--quiet", full_ref])
        .with_context(|| format!("run git show-ref in {}", repo_root.display()))?;
    if output.status.success() {
        return Ok(true);
    }
    match output.status.code() {
        Some(1) => Ok(false),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git show-ref failed: {}", stderr.trim())
        }
    }
}

fn checkout_branch_from_base(workspace_path: &Path, branch: &str, base_ref: &str) -> Result<()> {
    let output = git_output(workspace_path, &["checkout", "-B", branch, base_ref])
        .with_context(|| format!("run git checkout -B in {}", workspace_path.display()))?;
    ensure_git_success(
        output.status.success(),
        "git checkout -B failed",
        &output.stderr,
    )
}

fn hard_reset_and_clean(workspace_path: &Path, base_ref: &str) -> Result<()> {
    let reset_output = git_output(workspace_path, &["reset", "--hard", base_ref])
        .with_context(|| format!("run git reset in {}", workspace_path.display()))?;
    ensure_git_success(
        reset_output.status.success(),
        "git reset --hard failed",
        &reset_output.stderr,
    )?;

    let clean_output = git_output(workspace_path, &["clean", "-fd"])
        .with_context(|| format!("run git clean in {}", workspace_path.display()))?;
    ensure_git_success(
        clean_output.status.success(),
        "git clean failed",
        &clean_output.stderr,
    )
}

fn ensure_git_success(success: bool, message: &str, stderr: &[u8]) -> Result<()> {
    if success {
        Ok(())
    } else {
        bail!("{message}: {}", String::from_utf8_lossy(stderr).trim())
    }
}
