//! GitHub CLI execution helpers for PR operations.
//!
//! Responsibilities:
//! - Run `gh` preflight checks and repo metadata lookups.
//! - Centralize PR-view command execution.
//! - Keep command execution concerns separate from status parsing.
//!
//! Not handled here:
//! - Higher-level PR create/merge workflows.
//! - Rendering or logging beyond managed-command diagnostics.
//!
//! Invariants/assumptions:
//! - Repo-scoped commands run from the target repository unless explicitly isolated elsewhere.
//! - `gh --version` and `gh auth status` are the preflight contract for availability checks.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Output;

use crate::git::github_cli::{gh_command, run_gh_command};
use crate::runutil::TimeoutClass;

use super::parse::{
    parse_name_with_owner_from_repo_view_json, parse_pr_view_json, should_fallback_to_merged_at,
};
use super::types::{FALLBACK_VIEW_FIELDS, PRIMARY_VIEW_FIELDS, PrViewJson};

pub(super) fn gh_repo_name_with_owner(repo_root: &Path) -> Result<String> {
    let mut command = gh_command(repo_root);
    command
        .arg("repo")
        .arg("view")
        .arg("--json")
        .arg("nameWithOwner");
    let output = run_gh_command(command, "gh repo view", TimeoutClass::GitHubCli, "gh")
        .with_context(|| format!("run gh repo view in {}", repo_root.display()))?;

    ensure_success("gh repo view", &output)?;
    parse_name_with_owner_from_repo_view_json(&output.stdout)
}

pub(super) fn pr_view_json(repo_root: &Path, selector: &str) -> Result<PrViewJson> {
    pr_view_json_with(repo_root, selector, |fields| {
        run_gh_pr_view(repo_root, selector, fields)
    })
}

pub(super) fn pr_view_json_with<F>(
    _repo_root: &Path,
    selector: &str,
    mut run_view: F,
) -> Result<PrViewJson>
where
    F: FnMut(&str) -> Result<PrViewJson>,
{
    match run_view(PRIMARY_VIEW_FIELDS) {
        Ok(json) => Ok(json),
        Err(error) => {
            if should_fallback_to_merged_at(&error) {
                return run_view(FALLBACK_VIEW_FIELDS).with_context(|| {
                    format!(
                        "gh pr view failed after falling back to mergedAt field for selector {selector}"
                    )
                });
            }
            Err(error).with_context(|| format!("load gh pr view for selector {selector}"))
        }
    }
}

pub(super) fn run_gh_pr_create(command: std::process::Command, repo_root: &Path) -> Result<Output> {
    let output = run_gh_command(command, "gh pr create", TimeoutClass::GitHubCli, "gh")
        .with_context(|| format!("run gh pr create in {}", repo_root.display()))?;
    ensure_success("gh pr create", &output)?;
    Ok(output)
}

pub(super) fn run_gh_pr_merge(
    command: std::process::Command,
    repo_name_with_owner: &str,
) -> Result<Output> {
    let output = run_gh_command(command, "gh pr merge", TimeoutClass::GitHubCli, "gh")
        .with_context(|| {
            format!("run gh pr merge --repo {repo_name_with_owner} in isolated cwd")
        })?;
    ensure_success("gh pr merge", &output)?;
    Ok(output)
}

pub(crate) fn check_gh_available() -> Result<()> {
    check_gh_available_with(run_gh_with_no_update)
}

pub(super) fn check_gh_available_with<F>(run_gh: F) -> Result<()>
where
    F: Fn(&[&str]) -> Result<Output>,
{
    let version_output = run_gh(&["--version"]).with_context(|| {
        "GitHub CLI (`gh`) not found on PATH. Install it from https://cli.github.com/ and re-run."
            .to_string()
    })?;

    if !version_output.status.success() {
        let stderr = String::from_utf8_lossy(&version_output.stderr);
        bail!(
            "`gh --version` failed (gh is not usable). Details: {}. Install/repair `gh` from https://cli.github.com/ and re-run.",
            stderr.trim()
        );
    }

    let auth_output = run_gh(&["auth", "status"]).with_context(|| {
        "Failed to run `gh auth status`. Ensure `gh` is properly installed.".to_string()
    })?;

    if !auth_output.status.success() {
        let stdout = String::from_utf8_lossy(&auth_output.stdout);
        let stderr = String::from_utf8_lossy(&auth_output.stderr);
        let details = if !stderr.is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        bail!(
            "GitHub CLI (`gh`) is not authenticated. Run `gh auth login` and re-run. Details: {}",
            details
        );
    }

    Ok(())
}

fn run_gh_pr_view(repo_root: &Path, selector: &str, fields: &str) -> Result<PrViewJson> {
    let mut command = gh_command(repo_root);
    command
        .arg("pr")
        .arg("view")
        .arg(selector)
        .arg("--json")
        .arg(fields);
    let output = run_gh_command(command, "gh pr view", TimeoutClass::GitHubCli, "gh")
        .with_context(|| format!("run gh pr view in {}", repo_root.display()))?;

    ensure_success("gh pr view", &output)?;
    parse_pr_view_json(&output.stdout)
}

fn run_gh_with_no_update(args: &[&str]) -> Result<Output> {
    let mut command = crate::git::github_cli::gh_command_in(&std::env::temp_dir());
    command.args(args);
    run_gh_command(
        command,
        format!("gh {}", args.join(" ")),
        TimeoutClass::Probe,
        "gh",
    )
    .with_context(|| format!("run gh {}", args.join(" ")))
}

fn ensure_success(command_name: &str, output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("{command_name} failed: {}", stderr.trim());
}
