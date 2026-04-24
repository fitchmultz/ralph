//! Git LFS tests.
//!
//! Purpose:
//! - Git LFS tests.
//!
//! Responsibilities:
//! - Cover focused LFS helpers and the aggregate health report.
//!
//! Not handled here:
//! - Higher-level git command flows outside the LFS subsystem.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests run in isolated temporary repositories.

use super::*;
use crate::testsupport::git as git_test;
use anyhow::Result;
use tempfile::TempDir;

#[test]
fn lfs_filter_status_reports_health() {
    let healthy = LfsFilterStatus {
        smudge_installed: true,
        clean_installed: true,
        smudge_value: Some("git-lfs smudge %f".to_string()),
        clean_value: Some("git-lfs clean %f".to_string()),
    };
    assert!(healthy.is_healthy());
    assert!(healthy.issues().is_empty());

    let unhealthy = LfsFilterStatus {
        smudge_installed: false,
        clean_installed: false,
        smudge_value: None,
        clean_value: None,
    };
    assert!(!unhealthy.is_healthy());
    assert_eq!(unhealthy.issues().len(), 2);
}

#[test]
fn lfs_status_summary_reports_issues() {
    let summary = LfsStatusSummary {
        staged_lfs: vec![],
        staged_not_lfs: vec!["large.bin".to_string()],
        unstaged_lfs: vec!["unstaged.bin".to_string()],
        untracked_attributes: vec![],
    };
    assert!(!summary.is_clean());
    let issues = summary.issue_descriptions();
    assert_eq!(issues.len(), 2);
}

#[test]
fn lfs_health_report_health_depends_on_subreports() {
    let report = LfsHealthReport {
        lfs_initialized: false,
        filter_status: None,
        status_summary: None,
        pointer_issues: vec![],
    };
    assert!(report.is_healthy());

    let report = LfsHealthReport {
        lfs_initialized: true,
        filter_status: None,
        status_summary: Some(LfsStatusSummary::default()),
        pointer_issues: vec![],
    };
    assert!(!report.is_healthy());
}

#[test]
fn validate_lfs_pointers_detects_invalid_pointer() -> Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    std::fs::write(temp.path().join("test.bin"), "invalid pointer content")?;

    let issues = validate_lfs_pointers(temp.path(), &["test.bin".to_string()])?;
    assert!(matches!(
        issues.as_slice(),
        [LfsPointerIssue::InvalidPointer { path, .. }] if path == "test.bin"
    ));
    Ok(())
}

#[test]
fn validate_lfs_pointers_skips_large_files() -> Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    std::fs::write(temp.path().join("large.bin"), vec![0u8; 2048])?;

    let issues = validate_lfs_pointers(temp.path(), &["large.bin".to_string()])?;
    assert!(issues.is_empty());
    Ok(())
}

#[test]
fn validate_lfs_pointers_accept_valid_pointer() -> Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    let pointer = "version https://git-lfs.github.com/spec/v1\noid sha256:abc123\nsize 123\n";
    std::fs::write(temp.path().join("valid.bin"), pointer)?;

    let issues = validate_lfs_pointers(temp.path(), &["valid.bin".to_string()])?;
    assert!(issues.is_empty());
    Ok(())
}

#[test]
fn lfs_pointer_issue_helpers_include_path() {
    let issue = LfsPointerIssue::InvalidPointer {
        path: "test/file.bin".to_string(),
        reason: "corrupted".to_string(),
    };
    assert_eq!(issue.path(), "test/file.bin");
    assert!(issue.description().contains("corrupted"));
}

#[test]
fn check_lfs_health_errors_when_detected_repo_has_broken_git_config() {
    let temp = TempDir::new().expect("tempdir");
    git_test::init_repo(temp.path()).expect("init repo");
    std::fs::write(temp.path().join(".gitattributes"), "*.bin filter=lfs\n")
        .expect("write gitattributes");
    std::fs::create_dir_all(temp.path().join(".git/lfs")).expect("create lfs dir");
    std::fs::write(temp.path().join(".git/config"), "not a valid config")
        .expect("write invalid config");

    let err = check_lfs_health(temp.path()).expect_err("health should fail");
    let message = format!("{err:#}");
    assert!(
        message.to_lowercase().contains("git") || message.to_lowercase().contains("config"),
        "unexpected error: {message}"
    );
}
