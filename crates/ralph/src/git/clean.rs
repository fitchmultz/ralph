//! Repository cleanliness validation.
//!
//! This module provides functions for validating that a repository is in a clean
//! state, with support for allowing specific paths to be dirty (e.g., Ralph's
//! own configuration files).
//!
//! # Invariants
//! - Allowed paths must be normalized before comparison
//! - Directory prefixes work with or without trailing slashes
//! - Force flag bypasses all checks
//!
//! # What this does NOT handle
//! - Actual git operations (see git/commit.rs)
//! - Status parsing details (see git/status.rs)
//! - LFS validation (see git/lfs.rs)

use crate::git::error::GitError;
use crate::git::status::{parse_porcelain_z_entries, status_porcelain};
use std::path::Path;

/// Paths that are allowed to be dirty during Ralph runs.
///
/// These are Ralph's own configuration and state files that may change
/// during normal operation.
pub const RALPH_RUN_CLEAN_ALLOWED_PATHS: &[&str] = &[
    ".ralph/queue.json",
    ".ralph/done.json",
    ".ralph/config.json",
];

/// Require a clean repository, ignoring allowed paths.
///
/// Returns an error if the repository has uncommitted changes outside
/// the allowed paths. The force flag bypasses this check entirely.
///
/// # Arguments
/// * `repo_root` - Path to the repository root
/// * `force` - If true, bypass the check entirely
/// * `allowed_paths` - Paths that are allowed to be dirty
///
/// # Returns
/// * `Ok(())` - Repository is clean or force was true
/// * `Err(GitError::DirtyRepo)` - Repository has disallowed changes
pub fn require_clean_repo_ignoring_paths(
    repo_root: &Path,
    force: bool,
    allowed_paths: &[&str],
) -> Result<(), GitError> {
    let status = status_porcelain(repo_root)?;
    if status.trim().is_empty() {
        return Ok(());
    }

    if force {
        return Ok(());
    }

    let mut tracked = Vec::new();
    let mut untracked = Vec::new();

    let entries = parse_porcelain_z_entries(&status)?;
    for entry in entries {
        let path = entry.path.as_str();
        if !path_is_allowed(repo_root, path, allowed_paths) {
            let display = format_porcelain_entry(&entry);
            if entry.xy == "??" {
                untracked.push(display);
            } else {
                tracked.push(display);
            }
        }
    }

    if tracked.is_empty() && untracked.is_empty() {
        return Ok(());
    }

    let mut details = String::new();

    if !tracked.is_empty() {
        details.push_str("\n\nTracked changes (suggest 'git stash' or 'git commit'):");
        for line in tracked.iter().take(10) {
            details.push_str("\n  ");
            details.push_str(line);
        }
        if tracked.len() > 10 {
            details.push_str(&format!("\n  ...and {} more", tracked.len() - 10));
        }
    }

    if !untracked.is_empty() {
        details.push_str("\n\nUntracked files (suggest 'git clean -fd' or 'git add'):");
        for line in untracked.iter().take(10) {
            details.push_str("\n  ");
            details.push_str(line);
        }
        if untracked.len() > 10 {
            details.push_str(&format!("\n  ...and {} more", untracked.len() - 10));
        }
    }

    details.push_str("\n\nUse --force to bypass this check if you are sure.");
    Err(GitError::DirtyRepo { details })
}

/// Returns true when the repo has dirty paths and every dirty path is allowed.
///
/// This is useful for detecting if only Ralph's own files have changed.
pub fn repo_dirty_only_allowed_paths(
    repo_root: &Path,
    allowed_paths: &[&str],
) -> Result<bool, GitError> {
    use crate::git::status::status_paths;

    let status_paths = status_paths(repo_root)?;
    if status_paths.is_empty() {
        return Ok(false);
    }

    let has_disallowed = status_paths
        .iter()
        .any(|path| !path_is_allowed(repo_root, path, allowed_paths));
    Ok(!has_disallowed)
}

/// Check if a path is allowed to be dirty.
///
/// Handles normalization of paths and directory prefix matching.
fn path_is_allowed(repo_root: &Path, path: &str, allowed_paths: &[&str]) -> bool {
    let Some(normalized) = normalize_path_value(path) else {
        return false;
    };

    let normalized_dir = if normalized.ends_with('/') {
        normalized.to_string()
    } else {
        format!("{}/", normalized)
    };
    let normalized_is_dir = repo_root.join(normalized).is_dir();

    allowed_paths.iter().any(|allowed| {
        let Some(allowed_norm) = normalize_path_value(allowed) else {
            return false;
        };

        if normalized == allowed_norm {
            return true;
        }

        let is_dir_prefix = allowed_norm.ends_with('/') || repo_root.join(allowed_norm).is_dir();
        if !is_dir_prefix {
            return false;
        }

        let allowed_dir = allowed_norm.trim_end_matches('/');
        if allowed_dir.is_empty() {
            return false;
        }

        if normalized == allowed_dir {
            return true;
        }

        let prefix = format!("{}/", allowed_dir);
        if normalized.starts_with(&prefix) || normalized_dir.starts_with(&prefix) {
            return true;
        }

        let allowed_dir_slash = prefix;
        normalized_is_dir && allowed_dir_slash.starts_with(&normalized_dir)
    })
}

/// Normalize a path value for comparison.
fn normalize_path_value(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.strip_prefix("./").unwrap_or(trimmed))
}

/// Format a porcelain entry for display.
fn format_porcelain_entry(entry: &crate::git::status::PorcelainZEntry) -> String {
    if let Some(old) = entry.old_path.as_deref() {
        format!("{} {} -> {}", entry.xy, old, entry.path)
    } else {
        format!("{} {}", entry.xy, entry.path)
    }
}

#[cfg(test)]
mod clean_repo_tests {
    use super::*;
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;

    #[test]
    fn repo_dirty_only_allowed_paths_detects_config_only_changes() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::create_dir_all(temp.path().join(".ralph"))?;
        let config_path = temp.path().join(".ralph/config.json");
        std::fs::write(&config_path, "{ \"version\": 1 }")?;
        git_test::git_run(temp.path(), &["add", "-f", ".ralph/config.json"])?;
        git_test::git_run(temp.path(), &["commit", "-m", "init config"])?;

        std::fs::write(&config_path, "{ \"version\": 2 }")?;

        let dirty_allowed =
            repo_dirty_only_allowed_paths(temp.path(), RALPH_RUN_CLEAN_ALLOWED_PATHS)?;
        assert!(dirty_allowed, "expected config-only changes to be allowed");
        require_clean_repo_ignoring_paths(temp.path(), false, RALPH_RUN_CLEAN_ALLOWED_PATHS)?;
        Ok(())
    }

    #[test]
    fn repo_dirty_only_allowed_paths_rejects_other_changes() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("notes.txt"), "hello")?;

        let dirty_allowed =
            repo_dirty_only_allowed_paths(temp.path(), RALPH_RUN_CLEAN_ALLOWED_PATHS)?;
        assert!(!dirty_allowed, "expected untracked change to be disallowed");
        Ok(())
    }

    #[test]
    fn repo_dirty_only_allowed_paths_accepts_directory_prefix_with_trailing_slash(
    ) -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::create_dir_all(temp.path().join("cache/plans"))?;
        std::fs::write(temp.path().join("cache/plans/plan.md"), "plan")?;

        let dirty_allowed = repo_dirty_only_allowed_paths(temp.path(), &["cache/plans/"])?;
        assert!(dirty_allowed, "expected directory prefix to be allowed");
        require_clean_repo_ignoring_paths(temp.path(), false, &["cache/plans/"])?;
        Ok(())
    }

    #[test]
    fn repo_dirty_only_allowed_paths_accepts_existing_directory_prefix_without_slash(
    ) -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::create_dir_all(temp.path().join("cache"))?;
        std::fs::write(temp.path().join("cache/notes.txt"), "notes")?;

        let dirty_allowed = repo_dirty_only_allowed_paths(temp.path(), &["cache"])?;
        assert!(dirty_allowed, "expected existing directory to be allowed");
        require_clean_repo_ignoring_paths(temp.path(), false, &["cache"])?;
        Ok(())
    }

    #[test]
    fn repo_dirty_only_allowed_paths_rejects_paths_outside_allowed_directory() -> anyhow::Result<()>
    {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::create_dir_all(temp.path().join("cache"))?;
        std::fs::write(temp.path().join("cache/notes.txt"), "notes")?;
        std::fs::write(temp.path().join("other.txt"), "nope")?;

        let dirty_allowed = repo_dirty_only_allowed_paths(temp.path(), &["cache/"])?;
        assert!(!dirty_allowed, "expected other paths to be disallowed");
        assert!(
            require_clean_repo_ignoring_paths(temp.path(), false, &["cache/"]).is_err(),
            "expected clean-repo enforcement to fail"
        );
        Ok(())
    }
}
