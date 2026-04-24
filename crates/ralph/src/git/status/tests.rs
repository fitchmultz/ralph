//! Regression tests for git porcelain parsing and ignored-path helpers.
//!
//! Purpose:
//! - Regression tests for git porcelain parsing and ignored-path helpers.
//!
//! Responsibilities:
//! - Cover porcelain `-z` parsing edge cases.
//! - Verify ignored-path detection against a temp repository.
//!
//! Not handled here:
//! - Commit/push behavior.
//! - Repository cleanliness policy.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Trailing or consecutive NUL fields must not truncate porcelain parsing.
//! - Ignored-path helpers should reflect git's own ignore rules.

use super::*;
use crate::testsupport::git as git_test;
use tempfile::TempDir;

#[test]
fn parse_porcelain_z_entries_skips_empty_fields_including_trailing_nuls() -> Result<()> {
    let status = "?? file1\0\0?? file2\0\0";
    let entries = parse_porcelain_z_entries(status)?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].xy, "??");
    assert_eq!(entries[0].path, "file1");
    assert_eq!(entries[1].xy, "??");
    assert_eq!(entries[1].path, "file2");
    Ok(())
}

#[test]
fn parse_porcelain_z_entries_parses_copy_entries() -> Result<()> {
    let status = "C  new name.txt\0old name.txt\0";
    let entries = parse_porcelain_z_entries(status)?;
    assert_eq!(
        entries,
        vec![PorcelainZEntry {
            xy: "C ".to_string(),
            old_path: Some("old name.txt".to_string()),
            path: "new name.txt".to_string(),
        }]
    );
    Ok(())
}

#[test]
fn ignored_paths_lists_gitignored_entries() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    std::fs::write(repo_root.join(".gitignore"), ".env\nignored_dir/\n")?;
    std::fs::write(repo_root.join(".env"), "secret")?;
    std::fs::create_dir_all(repo_root.join("ignored_dir"))?;
    std::fs::write(repo_root.join("ignored_dir/file.txt"), "ignored content")?;

    let ignored = ignored_paths(&repo_root)?;

    assert!(ignored.contains(&".env".to_string()));
    assert!(ignored.contains(&"ignored_dir/".to_string()));
    Ok(())
}

#[test]
fn ignored_paths_errors_outside_repo() {
    let temp = TempDir::new().expect("temp dir");
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("create dir");

    let err = ignored_paths(&repo_root).expect_err("should fail outside repo");
    assert!(matches!(err, GitError::CommandFailed { .. }));
}
