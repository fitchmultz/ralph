//! GitHub PR helpers using the `gh` CLI.
//!
//! Responsibilities:
//! - Create PRs for worker branches and return structured metadata.
//! - Merge PRs using a chosen merge method.
//! - Query PR mergeability state.
//!
//! Not handled here:
//! - Task selection or worker execution (see `commands::run::parallel`).
//! - Conflict resolution logic (see `commands::run::parallel::merge_runner`).
//!
//! Invariants/assumptions:
//! - `gh` is installed and authenticated.
//! - Repo root points to a GitHub-backed repository.

use crate::contracts::ParallelMergeMethod;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub(crate) struct PrInfo {
    pub number: u32,
    pub url: String,
    pub head: String,
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MergeState {
    Clean,
    Dirty,
    Other(String),
}

#[derive(Deserialize)]
struct PrViewJson {
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: String,
    number: Option<u32>,
    url: Option<String>,
    #[serde(rename = "headRefName")]
    head: Option<String>,
    #[serde(rename = "baseRefName")]
    base: Option<String>,
}

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

    let body = if body.trim().is_empty() {
        "Automated by Ralph.".to_string()
    } else {
        body.to_string()
    };

    let mut cmd = Command::new("gh");
    cmd.current_dir(repo_root);
    cmd.arg("pr")
        .arg("create")
        .arg("--title")
        .arg(safe_title)
        .arg("--body")
        .arg(body)
        .arg("--head")
        .arg(head)
        .arg("--base")
        .arg(base);
    if draft {
        cmd.arg("--draft");
    }

    let output = cmd
        .output()
        .with_context(|| format!("run gh pr create in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh pr create failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let pr_url = extract_pr_url(&stdout).ok_or_else(|| {
        anyhow::anyhow!(
            "Unable to parse PR URL from gh output. Output: {}",
            stdout.trim()
        )
    })?;

    pr_view(repo_root, &pr_url)
}

pub(crate) fn merge_pr(
    repo_root: &Path,
    pr_number: u32,
    method: ParallelMergeMethod,
    delete_branch: bool,
) -> Result<()> {
    let mut cmd = Command::new("gh");
    cmd.current_dir(repo_root);
    cmd.arg("pr").arg("merge").arg(pr_number.to_string());

    match method {
        ParallelMergeMethod::Squash => {
            cmd.arg("--squash");
        }
        ParallelMergeMethod::Merge => {
            cmd.arg("--merge");
        }
        ParallelMergeMethod::Rebase => {
            cmd.arg("--rebase");
        }
    }

    if delete_branch {
        cmd.arg("--delete-branch");
    }

    let output = cmd
        .output()
        .with_context(|| format!("run gh pr merge in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh pr merge failed: {}", stderr.trim());
    }

    Ok(())
}

pub(crate) fn pr_merge_state(repo_root: &Path, pr_number: u32) -> Result<MergeState> {
    let json = pr_view_json(repo_root, &pr_number.to_string())?;
    Ok(match json.merge_state_status.as_str() {
        "CLEAN" => MergeState::Clean,
        "DIRTY" => MergeState::Dirty,
        other => MergeState::Other(other.to_string()),
    })
}

fn pr_view(repo_root: &Path, selector: &str) -> Result<PrInfo> {
    let json = pr_view_json(repo_root, selector)?;
    let number = json
        .number
        .ok_or_else(|| anyhow::anyhow!("Missing PR number in gh response"))?;
    let url = json
        .url
        .ok_or_else(|| anyhow::anyhow!("Missing PR url in gh response"))?;
    let head = json
        .head
        .ok_or_else(|| anyhow::anyhow!("Missing PR head in gh response"))?;
    let base = json
        .base
        .ok_or_else(|| anyhow::anyhow!("Missing PR base in gh response"))?;

    Ok(PrInfo {
        number,
        url,
        head,
        base,
    })
}

fn pr_view_json(repo_root: &Path, selector: &str) -> Result<PrViewJson> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .arg("pr")
        .arg("view")
        .arg(selector)
        .arg("--json")
        .arg("mergeStateStatus,number,url,headRefName,baseRefName")
        .output()
        .with_context(|| format!("run gh pr view in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh pr view failed: {}", stderr.trim());
    }

    let json: PrViewJson =
        serde_json::from_slice(&output.stdout).context("parse gh pr view json")?;
    Ok(json)
}

fn extract_pr_url(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(|line| line.to_string())
}

#[cfg(test)]
mod tests {
    use super::extract_pr_url;

    #[test]
    fn extract_pr_url_picks_first_url_line() {
        let output = "Creating pull request for feature...\nhttps://github.com/org/repo/pull/5\n";
        let url = extract_pr_url(output).expect("url");
        assert_eq!(url, "https://github.com/org/repo/pull/5");
    }
}
