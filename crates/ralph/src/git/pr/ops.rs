//! High-level PR workflows.
//!
//! Purpose:
//! - High-level PR workflows.
//!
//! Responsibilities:
//! - Build and execute `gh` commands for create/merge/status operations.
//! - Map CLI output into crate-facing PR models.
//! - Keep user-visible behavior stable while delegating parsing/execution internals.
//!
//! Not handled here:
//! - Shared `gh` execution plumbing.
//! - Raw JSON parsing and lifecycle derivation details.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Empty PR titles are rejected before invoking `gh`.
//! - Merge operations run from an isolated cwd plus explicit `--repo`.

use anyhow::{Result, anyhow, bail};
use std::path::Path;

use crate::git::github_cli::{extract_first_url, gh_command, gh_command_in};

use super::gh::{gh_repo_name_with_owner, pr_view_json, run_gh_pr_create, run_gh_pr_merge};
use super::parse::{pr_info_from_view, pr_lifecycle_status_from_view, pr_merge_status_from_view};
use super::types::{MergeMethod, PrInfo, PrMergeStatus};

pub(crate) fn create_pr(
    repo_root: &Path,
    title: &str,
    body: &str,
    head: &str,
    base: &str,
    draft: bool,
) -> Result<PrInfo> {
    let safe_title = title.trim();
    if safe_title.is_empty() {
        bail!("PR title must be non-empty");
    }

    let mut command = gh_command(repo_root);
    command
        .arg("pr")
        .arg("create")
        .arg("--title")
        .arg(safe_title)
        .arg("--body")
        .arg(normalized_body(body))
        .arg("--head")
        .arg(head)
        .arg("--base")
        .arg(base);
    if draft {
        command.arg("--draft");
    }

    let output = run_gh_pr_create(command, repo_root)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let pr_url = extract_first_url(&stdout).ok_or_else(|| {
        anyhow!(
            "Unable to parse PR URL from gh output. Output: {}",
            stdout.trim()
        )
    })?;

    pr_view(repo_root, &pr_url)
}

pub(crate) fn merge_pr(
    repo_root: &Path,
    pr_number: u32,
    method: MergeMethod,
    delete_branch: bool,
) -> Result<()> {
    let repo_name_with_owner = gh_repo_name_with_owner(repo_root)?;
    let mut command = gh_command_in(&std::env::temp_dir());
    command
        .arg("pr")
        .arg("merge")
        .arg(pr_number.to_string())
        .arg("--repo")
        .arg(&repo_name_with_owner)
        .arg(merge_method_flag(method));

    if delete_branch {
        command.arg("--delete-branch");
    }

    run_gh_pr_merge(command, &repo_name_with_owner)?;
    Ok(())
}

pub(crate) fn pr_merge_status(repo_root: &Path, pr_number: u32) -> Result<PrMergeStatus> {
    let json = pr_view_json(repo_root, &pr_number.to_string())?;
    Ok(pr_merge_status_from_view(&json))
}

pub(crate) fn pr_lifecycle_status(
    repo_root: &Path,
    pr_number: u32,
) -> Result<super::types::PrLifecycleStatus> {
    let json = pr_view_json(repo_root, &pr_number.to_string())?;
    Ok(pr_lifecycle_status_from_view(&json))
}

fn pr_view(repo_root: &Path, selector: &str) -> Result<PrInfo> {
    pr_info_from_view(pr_view_json(repo_root, selector)?)
}

pub(super) fn merge_method_flag(method: MergeMethod) -> &'static str {
    match method {
        MergeMethod::Squash => "--squash",
        MergeMethod::Merge => "--merge",
        MergeMethod::Rebase => "--rebase",
    }
}

fn normalized_body(body: &str) -> &str {
    if body.trim().is_empty() {
        "Automated by Ralph."
    } else {
        body
    }
}
