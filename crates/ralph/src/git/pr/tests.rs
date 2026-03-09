//! Tests for GitHub PR helpers.
//!
//! Responsibilities:
//! - Cover URL parsing, status/lifecycle derivation, and gh preflight behavior.
//! - Lock in fallback behavior for older `gh pr view` JSON field support.
//! - Keep PR module regression tests near the implementation split.
//!
//! Not handled here:
//! - End-to-end integration with a live GitHub repository.
//! - Managed subprocess behavior already covered elsewhere.
//!
//! Invariants/assumptions:
//! - Tests simulate `gh` responses via injected closures instead of spawning `gh`.

use super::gh::{check_gh_available_with, pr_view_json_with};
use super::ops::merge_method_flag;
use super::parse::{
    parse_name_with_owner_from_repo_view_json, pr_lifecycle_status_from_view,
    pr_merge_status_from_view,
};
use super::types::{MergeMethod, MergeState, PrLifecycle, PrViewJson};

#[test]
fn merge_method_flag_matches_expected_cli_args() {
    assert_eq!(merge_method_flag(MergeMethod::Squash), "--squash");
    assert_eq!(merge_method_flag(MergeMethod::Merge), "--merge");
    assert_eq!(merge_method_flag(MergeMethod::Rebase), "--rebase");
}

#[test]
fn pr_merge_status_from_view_tracks_draft_flag() {
    let json = sample_view("CLEAN", "OPEN");
    let status = pr_merge_status_from_view(&json);
    assert_eq!(status.merge_state, MergeState::Clean);
    assert!(status.is_draft);
}

#[test]
fn pr_merge_status_from_view_defaults_draft_false() {
    let mut json = sample_view("DIRTY", "OPEN");
    json.is_draft = None;
    let status = pr_merge_status_from_view(&json);
    assert_eq!(status.merge_state, MergeState::Dirty);
    assert!(!status.is_draft);
}

#[test]
fn pr_merge_status_from_view_handles_unknown_state() {
    let mut json = sample_view("BLOCKED", "OPEN");
    json.is_draft = Some(false);
    let status = pr_merge_status_from_view(&json);
    assert_eq!(status.merge_state, MergeState::Other("BLOCKED".to_string()));
    assert!(!status.is_draft);
}

#[test]
fn pr_lifecycle_status_from_view_open() {
    let mut json = sample_view("CLEAN", "OPEN");
    json.is_draft = Some(false);
    let status = pr_lifecycle_status_from_view(&json);
    assert!(matches!(status.lifecycle, PrLifecycle::Open));
    assert!(!status.is_merged);
}

#[test]
fn pr_lifecycle_status_from_view_closed_not_merged() {
    let mut json = sample_view("CLEAN", "CLOSED");
    json.is_draft = Some(false);
    json.is_merged = Some(false);
    let status = pr_lifecycle_status_from_view(&json);
    assert!(matches!(status.lifecycle, PrLifecycle::Closed));
    assert!(!status.is_merged);
}

#[test]
fn pr_lifecycle_status_from_view_closed_merged_at() {
    let mut json = sample_view("CLEAN", "CLOSED");
    json.is_draft = Some(false);
    json.is_merged = None;
    json.merged_at = Some("2026-01-19T00:00:00Z".to_string());
    let status = pr_lifecycle_status_from_view(&json);
    assert!(matches!(status.lifecycle, PrLifecycle::Merged));
    assert!(status.is_merged);
}

#[test]
fn pr_lifecycle_status_from_view_closed_merged() {
    let mut json = sample_view("CLEAN", "CLOSED");
    json.is_draft = Some(false);
    json.is_merged = Some(true);
    let status = pr_lifecycle_status_from_view(&json);
    assert!(matches!(status.lifecycle, PrLifecycle::Merged));
    assert!(status.is_merged);
}

#[test]
fn pr_lifecycle_status_from_view_merged_state() {
    let mut json = sample_view("CLEAN", "MERGED");
    json.is_draft = Some(false);
    json.is_merged = Some(true);
    let status = pr_lifecycle_status_from_view(&json);
    assert!(matches!(status.lifecycle, PrLifecycle::Merged));
    assert!(status.is_merged);
}

#[test]
fn pr_lifecycle_status_from_view_unknown_state() {
    let mut json = sample_view("CLEAN", "WEIRD");
    json.is_draft = Some(false);
    json.is_merged = Some(false);
    let status = pr_lifecycle_status_from_view(&json);
    assert!(matches!(status.lifecycle, PrLifecycle::Unknown(ref s) if s == "WEIRD"));
    assert!(!status.is_merged);
}

#[test]
fn pr_view_json_with_falls_back_to_merged_at_field() {
    let repo_root = std::path::Path::new("/tmp/repo");
    let mut calls = Vec::new();
    let json = pr_view_json_with(repo_root, "123", |fields| {
        calls.push(fields.to_string());
        if fields.contains("merged,") || fields.ends_with("merged") {
            anyhow::bail!("Unknown JSON field: \"merged\"");
        }
        Ok(sample_view("CLEAN", "OPEN"))
    })
    .expect("fallback should succeed");

    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], super::types::PRIMARY_VIEW_FIELDS);
    assert_eq!(calls[1], super::types::FALLBACK_VIEW_FIELDS);
    assert_eq!(json.number, Some(1));
}

#[test]
fn check_gh_available_fails_when_gh_not_found() {
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

    assert!(check_gh_available_with(run_gh).is_ok());
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

fn sample_view(merge_state_status: &str, state: &str) -> PrViewJson {
    PrViewJson {
        merge_state_status: merge_state_status.to_string(),
        number: Some(1),
        url: Some("https://example.com/pr/1".to_string()),
        head: Some("ralph/RQ-0001".to_string()),
        base: Some("main".to_string()),
        is_draft: Some(true),
        state: Some(state.to_string()),
        is_merged: Some(false),
        merged_at: None,
    }
}
