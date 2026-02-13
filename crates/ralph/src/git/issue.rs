//! GitHub Issue helpers using the `gh` CLI.
//!
//! Responsibilities:
//! - Create and edit GitHub issues for Ralph tasks via `gh issue`.
//! - Parse issue URLs/numbers from `gh` output for persistence.
//!
//! Not handled here:
//! - Queue mutation or task persistence.
//! - Rendering issue bodies from tasks (see `cli::queue::export`).
//!
//! Invariants/assumptions:
//! - `gh` is installed and authenticated.
//! - Commands run with `GH_NO_UPDATE_NOTIFIER=1` to avoid noisy prompts.

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

pub(crate) const GITHUB_ISSUE_SYNC_HASH_KEY: &str = "github_issue_sync_hash";

pub(crate) struct IssueInfo {
    pub url: String,
    pub number: Option<u32>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
struct IssueSyncPayload<'a> {
    title: &'a str,
    body: &'a str,
    labels: Vec<String>,
    assignees: Vec<String>,
    repo: Option<&'a str>,
}

pub(crate) fn normalize_issue_metadata_list(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values
}

pub(crate) fn compute_issue_sync_hash(
    title: &str,
    body: &str,
    labels: &[String],
    assignees: &[String],
    repo: Option<&str>,
) -> Result<String> {
    let payload = IssueSyncPayload {
        title: title.trim(),
        body: body.trim(),
        labels: normalize_issue_metadata_list(labels),
        assignees: normalize_issue_metadata_list(assignees),
        repo: repo.map(str::trim).filter(|r| !r.is_empty()),
    };

    let encoded = serde_json::to_string(&payload)
        .context("failed to serialize issue sync fingerprint payload")?;
    let mut hasher = Sha256::new();
    hasher.update(encoded.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

fn extract_first_url(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(|line| line.to_string())
}

pub(crate) fn parse_issue_number(url: &str) -> Option<u32> {
    let marker = "/issues/";
    let idx = url.find(marker)?;
    let rest = &url[idx + marker.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

pub(crate) fn create_issue(
    repo_root: &Path,
    selector_repo: Option<&str>,
    title: &str,
    body_file: &Path,
    labels: &[String],
    assignees: &[String],
) -> Result<IssueInfo> {
    let safe_title = title.trim();
    if safe_title.is_empty() {
        bail!("Issue title must be non-empty");
    }

    let mut cmd = Command::new("gh");
    cmd.current_dir(repo_root)
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .arg("issue")
        .arg("create")
        .arg("--title")
        .arg(safe_title)
        .arg("--body-file")
        .arg(body_file);

    if let Some(repo) = selector_repo {
        cmd.arg("-R").arg(repo);
    }

    for label in labels {
        cmd.arg("--label").arg(label);
    }
    for assignee in assignees {
        cmd.arg("--assignee").arg(assignee);
    }

    let output = cmd
        .output()
        .with_context(|| format!("run gh issue create in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh issue create failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = extract_first_url(&stdout).ok_or_else(|| {
        anyhow::anyhow!(
            "Unable to parse issue URL from gh output. Output: {}",
            stdout.trim()
        )
    })?;

    Ok(IssueInfo {
        number: parse_issue_number(&url),
        url,
    })
}

pub(crate) fn edit_issue(
    repo_root: &Path,
    selector_repo: Option<&str>,
    issue_selector: &str, // number or URL
    title: &str,
    body_file: &Path,
    add_labels: &[String],
    add_assignees: &[String],
) -> Result<()> {
    let safe_title = title.trim();
    if safe_title.is_empty() {
        bail!("Issue title must be non-empty");
    }

    let mut cmd = Command::new("gh");
    cmd.current_dir(repo_root)
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .arg("issue")
        .arg("edit")
        .arg(issue_selector)
        .arg("--title")
        .arg(safe_title)
        .arg("--body-file")
        .arg(body_file);

    if let Some(repo) = selector_repo {
        cmd.arg("-R").arg(repo);
    }

    for label in add_labels {
        cmd.arg("--add-label").arg(label);
    }
    for assignee in add_assignees {
        cmd.arg("--add-assignee").arg(assignee);
    }

    let output = cmd
        .output()
        .with_context(|| format!("run gh issue edit in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh issue edit failed: {}", stderr.trim());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{extract_first_url, parse_issue_number};

    #[test]
    fn extract_first_url_picks_first_url_line() {
        let output = "Creating issue for task...\nhttps://github.com/org/repo/issues/5\n";
        let url = extract_first_url(output).expect("url");
        assert_eq!(url, "https://github.com/org/repo/issues/5");
    }

    #[test]
    fn extract_first_url_returns_none_when_no_url() {
        let output = "Some output without a URL\n";
        assert!(extract_first_url(output).is_none());
    }

    #[test]
    fn parse_issue_number_extracts_number() {
        assert_eq!(
            parse_issue_number("https://github.com/org/repo/issues/123"),
            Some(123)
        );
        assert_eq!(
            parse_issue_number("https://github.com/org/repo/issues/42?foo=bar"),
            Some(42)
        );
    }

    #[test]
    fn parse_issue_number_returns_none_for_invalid() {
        assert!(parse_issue_number("https://github.com/org/repo/pull/123").is_none());
        assert!(parse_issue_number("not a url").is_none());
        assert!(parse_issue_number("").is_none());
    }
}
