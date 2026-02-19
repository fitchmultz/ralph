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
    #[allow(dead_code)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrMergeStatus {
    pub merge_state: MergeState,
    pub is_draft: bool,
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
    #[serde(rename = "isDraft")]
    is_draft: Option<bool>,
    state: Option<String>,
    #[serde(rename = "merged")]
    is_merged: Option<bool>,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
}

#[derive(Deserialize)]
struct RepoViewNameWithOwnerJson {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}

/// PR lifecycle states as returned by GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PrLifecycle {
    Open,
    Closed,
    Merged,
    Unknown(String),
}

/// PR lifecycle status including lifecycle and merged flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrLifecycleStatus {
    pub lifecycle: PrLifecycle,
    pub is_merged: bool,
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
    let repo_name_with_owner = gh_repo_name_with_owner(repo_root)?;

    let mut cmd = Command::new("gh");
    // Use an isolated cwd plus explicit --repo to prevent gh from mutating the
    // coordinator working tree during merge operations.
    cmd.current_dir(std::env::temp_dir());
    cmd.arg("pr")
        .arg("merge")
        .arg(pr_number.to_string())
        .arg("--repo")
        .arg(&repo_name_with_owner)
        .arg(merge_method_flag(method));

    if delete_branch {
        cmd.arg("--delete-branch");
    }

    let output = cmd.output().with_context(|| {
        format!(
            "run gh pr merge --repo {} in isolated cwd",
            repo_name_with_owner
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh pr merge failed: {}", stderr.trim());
    }

    Ok(())
}

fn merge_method_flag(method: ParallelMergeMethod) -> &'static str {
    match method {
        ParallelMergeMethod::Squash => "--squash",
        ParallelMergeMethod::Merge => "--merge",
        ParallelMergeMethod::Rebase => "--rebase",
    }
}

fn gh_repo_name_with_owner(repo_root: &Path) -> Result<String> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .arg("repo")
        .arg("view")
        .arg("--json")
        .arg("nameWithOwner")
        .output()
        .with_context(|| format!("run gh repo view in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh repo view failed: {}", stderr.trim());
    }

    parse_name_with_owner_from_repo_view_json(&output.stdout)
}

fn parse_name_with_owner_from_repo_view_json(payload: &[u8]) -> Result<String> {
    let repo: RepoViewNameWithOwnerJson =
        serde_json::from_slice(payload).context("parse gh repo view json")?;
    let trimmed = repo.name_with_owner.trim();
    if trimmed.is_empty() {
        bail!("gh repo view returned empty nameWithOwner");
    }
    Ok(trimmed.to_string())
}

pub(crate) fn pr_merge_status(repo_root: &Path, pr_number: u32) -> Result<PrMergeStatus> {
    let json = pr_view_json(repo_root, &pr_number.to_string())?;
    Ok(pr_merge_status_from_view(&json))
}

/// Query PR lifecycle status from GitHub.
pub(crate) fn pr_lifecycle_status(repo_root: &Path, pr_number: u32) -> Result<PrLifecycleStatus> {
    let json = pr_view_json(repo_root, &pr_number.to_string())?;
    Ok(pr_lifecycle_status_from_view(&json))
}

fn pr_lifecycle_status_from_view(json: &PrViewJson) -> PrLifecycleStatus {
    let state = json.state.as_deref().unwrap_or("UNKNOWN");
    let merged_flag = json.is_merged.unwrap_or(false) || json.merged_at.as_ref().is_some();

    let lifecycle = match state {
        "OPEN" => PrLifecycle::Open,
        "CLOSED" => {
            if merged_flag {
                PrLifecycle::Merged
            } else {
                PrLifecycle::Closed
            }
        }
        "MERGED" => PrLifecycle::Merged,
        other => PrLifecycle::Unknown(other.to_string()),
    };

    let is_merged_final = merged_flag || matches!(lifecycle, PrLifecycle::Merged);

    PrLifecycleStatus {
        lifecycle,
        is_merged: is_merged_final,
    }
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
    let primary_fields = "mergeStateStatus,number,url,headRefName,baseRefName,isDraft,state,merged";
    match run_gh_pr_view(repo_root, selector, primary_fields) {
        Ok(json) => Ok(json),
        Err(err) => {
            let err_msg = err.to_string();
            if err_msg.contains("Unknown JSON field: \"merged\"") {
                let fallback_fields =
                    "mergeStateStatus,number,url,headRefName,baseRefName,isDraft,state,mergedAt";
                return run_gh_pr_view(repo_root, selector, fallback_fields).with_context(|| {
                    "gh pr view failed after falling back to mergedAt field".to_string()
                });
            }
            Err(err)
        }
    }
}

fn run_gh_pr_view(repo_root: &Path, selector: &str, fields: &str) -> Result<PrViewJson> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .arg("pr")
        .arg("view")
        .arg(selector)
        .arg("--json")
        .arg(fields)
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

fn pr_merge_status_from_view(json: &PrViewJson) -> PrMergeStatus {
    let merge_state = match json.merge_state_status.as_str() {
        "CLEAN" => MergeState::Clean,
        "DIRTY" => MergeState::Dirty,
        other => MergeState::Other(other.to_string()),
    };
    PrMergeStatus {
        merge_state,
        is_draft: json.is_draft.unwrap_or(false),
    }
}

fn extract_pr_url(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(|line| line.to_string())
}

/// Run a gh command with GH_NO_UPDATE_NOTIFIER set to avoid noisy updater prompts.
fn run_gh_with_no_update(args: &[&str]) -> Result<std::process::Output> {
    std::process::Command::new("gh")
        .args(args)
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .output()
        .with_context(|| format!("run gh {}", args.join(" ")))
}

/// Check if the GitHub CLI (`gh`) is available and authenticated.
///
/// This is intended for preflight checks before operations that require gh,
/// such as parallel mode with auto_pr or auto_merge enabled.
///
/// Returns Ok(()) if gh is on PATH and authenticated.
/// Returns an error with a clear, actionable message if gh is missing or not authenticated.
pub(crate) fn check_gh_available() -> Result<()> {
    check_gh_available_with(run_gh_with_no_update)
}

/// Internal implementation that accepts a custom gh runner for testability.
fn check_gh_available_with<F>(run_gh: F) -> Result<()>
where
    F: Fn(&[&str]) -> Result<std::process::Output>,
{
    // First, check if gh is on PATH by running --version
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

    // Then, check authentication status
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

#[cfg(test)]
mod tests {
    use crate::contracts::ParallelMergeMethod;

    use super::{MergeState, PrLifecycle, check_gh_available_with, extract_pr_url};
    use super::{
        PrViewJson, merge_method_flag, parse_name_with_owner_from_repo_view_json,
        pr_lifecycle_status_from_view, pr_merge_status_from_view,
    };

    #[test]
    fn extract_pr_url_picks_first_url_line() {
        let output = "Creating pull request for feature...\nhttps://github.com/org/repo/pull/5\n";
        let url = extract_pr_url(output).expect("url");
        assert_eq!(url, "https://github.com/org/repo/pull/5");
    }

    #[test]
    fn pr_merge_status_from_view_tracks_draft_flag() {
        let json = PrViewJson {
            merge_state_status: "CLEAN".to_string(),
            number: Some(1),
            url: Some("https://example.com/pr/1".to_string()),
            head: Some("ralph/RQ-0001".to_string()),
            base: Some("main".to_string()),
            is_draft: Some(true),
            state: Some("OPEN".to_string()),
            is_merged: Some(false),
            merged_at: None,
        };

        let status = pr_merge_status_from_view(&json);
        assert_eq!(status.merge_state, MergeState::Clean);
        assert!(status.is_draft);
    }

    #[test]
    fn pr_merge_status_from_view_defaults_draft_false() {
        let json = PrViewJson {
            merge_state_status: "DIRTY".to_string(),
            number: Some(2),
            url: Some("https://example.com/pr/2".to_string()),
            head: Some("ralph/RQ-0002".to_string()),
            base: Some("main".to_string()),
            is_draft: None,
            state: Some("OPEN".to_string()),
            is_merged: Some(false),
            merged_at: None,
        };

        let status = pr_merge_status_from_view(&json);
        assert_eq!(status.merge_state, MergeState::Dirty);
        assert!(!status.is_draft);
    }

    #[test]
    fn pr_merge_status_from_view_handles_unknown_state() {
        let json = PrViewJson {
            merge_state_status: "BLOCKED".to_string(),
            number: Some(3),
            url: Some("https://example.com/pr/3".to_string()),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            is_draft: Some(false),
            state: Some("OPEN".to_string()),
            is_merged: Some(false),
            merged_at: None,
        };

        let status = pr_merge_status_from_view(&json);
        assert_eq!(status.merge_state, MergeState::Other("BLOCKED".to_string()));
        assert!(!status.is_draft);
    }

    #[test]
    fn pr_lifecycle_status_from_view_open() {
        let json = PrViewJson {
            merge_state_status: "CLEAN".to_string(),
            number: Some(1),
            url: Some("https://example.com/pr/1".to_string()),
            head: Some("ralph/RQ-0001".to_string()),
            base: Some("main".to_string()),
            is_draft: Some(false),
            state: Some("OPEN".to_string()),
            is_merged: Some(false),
            merged_at: None,
        };

        let status = pr_lifecycle_status_from_view(&json);
        assert!(matches!(status.lifecycle, PrLifecycle::Open));
        assert!(!status.is_merged);
    }

    #[test]
    fn pr_lifecycle_status_from_view_closed_not_merged() {
        let json = PrViewJson {
            merge_state_status: "CLEAN".to_string(),
            number: Some(2),
            url: Some("https://example.com/pr/2".to_string()),
            head: Some("ralph/RQ-0002".to_string()),
            base: Some("main".to_string()),
            is_draft: Some(false),
            state: Some("CLOSED".to_string()),
            is_merged: Some(false),
            merged_at: None,
        };

        let status = pr_lifecycle_status_from_view(&json);
        assert!(matches!(status.lifecycle, PrLifecycle::Closed));
        assert!(!status.is_merged);
    }

    #[test]
    fn pr_lifecycle_status_from_view_closed_merged_at() {
        let json = PrViewJson {
            merge_state_status: "CLEAN".to_string(),
            number: Some(3),
            url: Some("https://example.com/pr/3".to_string()),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            is_draft: Some(false),
            state: Some("CLOSED".to_string()),
            is_merged: None,
            merged_at: Some("2026-01-19T00:00:00Z".to_string()),
        };

        let status = pr_lifecycle_status_from_view(&json);
        assert!(matches!(status.lifecycle, PrLifecycle::Merged));
        assert!(status.is_merged);
    }

    #[test]
    fn pr_lifecycle_status_from_view_closed_merged() {
        let json = PrViewJson {
            merge_state_status: "CLEAN".to_string(),
            number: Some(3),
            url: Some("https://example.com/pr/3".to_string()),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            is_draft: Some(false),
            state: Some("CLOSED".to_string()),
            is_merged: Some(true),
            merged_at: None,
        };

        let status = pr_lifecycle_status_from_view(&json);
        assert!(matches!(status.lifecycle, PrLifecycle::Merged));
        assert!(status.is_merged);
    }

    #[test]
    fn pr_lifecycle_status_from_view_merged_state() {
        let json = PrViewJson {
            merge_state_status: "CLEAN".to_string(),
            number: Some(4),
            url: Some("https://example.com/pr/4".to_string()),
            head: Some("ralph/RQ-0004".to_string()),
            base: Some("main".to_string()),
            is_draft: Some(false),
            state: Some("MERGED".to_string()),
            is_merged: Some(true),
            merged_at: None,
        };

        let status = pr_lifecycle_status_from_view(&json);
        assert!(matches!(status.lifecycle, PrLifecycle::Merged));
        assert!(status.is_merged);
    }

    #[test]
    fn pr_lifecycle_status_from_view_unknown_state() {
        let json = PrViewJson {
            merge_state_status: "CLEAN".to_string(),
            number: Some(5),
            url: Some("https://example.com/pr/5".to_string()),
            head: Some("ralph/RQ-0005".to_string()),
            base: Some("main".to_string()),
            is_draft: Some(false),
            state: Some("WEIRD".to_string()),
            is_merged: Some(false),
            merged_at: None,
        };

        let status = pr_lifecycle_status_from_view(&json);
        assert!(matches!(status.lifecycle, PrLifecycle::Unknown(s) if s == "WEIRD"));
        assert!(!status.is_merged);
    }

    #[test]
    fn check_gh_available_fails_when_gh_not_found() {
        // Simulate gh not being on PATH (io error)
        let run_gh = |_args: &[&str]| -> anyhow::Result<std::process::Output> {
            Err(anyhow::anyhow!(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No such file or directory"
            )))
        };

        let result = check_gh_available_with(run_gh);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("GitHub CLI (`gh`) not found on PATH"));
        assert!(msg.contains("https://cli.github.com/"));
    }

    #[test]
    fn check_gh_available_fails_when_version_fails() {
        // Simulate gh --version returning non-success
        // Get a failing exit status by running "false" command
        let fail_status = std::process::Command::new("false")
            .status()
            .expect("'false' command should exist");

        let run_gh = |args: &[&str]| -> anyhow::Result<std::process::Output> {
            if args == ["--version"] {
                Ok(std::process::Output {
                    status: fail_status,
                    stdout: vec![],
                    stderr: b"gh: command not recognized".to_vec(),
                })
            } else {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                })
            }
        };

        let result = check_gh_available_with(run_gh);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("`gh --version` failed"));
        assert!(msg.contains("gh is not usable"));
    }

    #[test]
    fn check_gh_available_fails_when_auth_fails() {
        // Simulate gh --version succeeding but auth status failing
        // Get a failing exit status by running "false" command
        let fail_status = std::process::Command::new("false")
            .status()
            .expect("'false' command should exist");

        let run_gh = |args: &[&str]| -> anyhow::Result<std::process::Output> {
            if args == ["--version"] {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::default(),
                    stdout: b"gh version 2.40.0".to_vec(),
                    stderr: vec![],
                })
            } else if args == ["auth", "status"] {
                Ok(std::process::Output {
                    status: fail_status,
                    stdout: vec![],
                    stderr: b"You are not logged into any GitHub hosts".to_vec(),
                })
            } else {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                })
            }
        };

        let result = check_gh_available_with(run_gh);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("GitHub CLI (`gh`) is not authenticated"));
        assert!(msg.contains("gh auth login"));
    }

    #[test]
    fn check_gh_available_succeeds_when_both_checks_pass() {
        // Simulate both gh --version and auth status succeeding
        let run_gh = |args: &[&str]| -> anyhow::Result<std::process::Output> {
            if args == ["--version"] {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::default(),
                    stdout: b"gh version 2.40.0".to_vec(),
                    stderr: vec![],
                })
            } else if args == ["auth", "status"] {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::default(),
                    stdout: b"Logged in to github.com as user".to_vec(),
                    stderr: vec![],
                })
            } else {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                })
            }
        };

        let result = check_gh_available_with(run_gh);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_name_with_owner_from_repo_view_json_accepts_valid_payload() {
        let payload = br#"{ "nameWithOwner": "org/repo" }"#;
        let result = parse_name_with_owner_from_repo_view_json(payload).expect("repo");
        assert_eq!(result, "org/repo");
    }

    #[test]
    fn parse_name_with_owner_from_repo_view_json_rejects_empty_value() {
        let payload = br#"{ "nameWithOwner": "   " }"#;
        let err = parse_name_with_owner_from_repo_view_json(payload).unwrap_err();
        assert!(
            err.to_string().contains("empty nameWithOwner"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn merge_method_flag_maps_all_variants() {
        assert_eq!(merge_method_flag(ParallelMergeMethod::Squash), "--squash");
        assert_eq!(merge_method_flag(ParallelMergeMethod::Merge), "--merge");
        assert_eq!(merge_method_flag(ParallelMergeMethod::Rebase), "--rebase");
    }
}
