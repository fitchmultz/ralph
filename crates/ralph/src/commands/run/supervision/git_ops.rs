//! Git operations for post-run supervision.
//!
//! Responsibilities:
//! - Finalize git state: commit changes and push if configured.
//! - Validate LFS configuration and warn about issues.
//! - Handle upstream push when ahead.
//!
//! Not handled here:
//! - Queue file operations (see queue_ops.rs).
//! - CI gate execution (see ci.rs).
//!
//! Invariants/assumptions:
//! - Git repo is initialized and accessible.
//! - LFS validation respects the strict flag for error vs warn behavior.

use crate::git;
use crate::git::GitError;
use crate::outpututil;
use anyhow::{Result, anyhow, bail};
use std::path::Path;

/// Handles the final git commit and push if enabled, and verifies the repo is clean.
pub(crate) fn finalize_git_state(
    resolved: &crate::config::Resolved,
    task_id: &str,
    task_title: &str,
    git_commit_push_enabled: bool,
) -> Result<()> {
    if git_commit_push_enabled {
        let commit_message = outpututil::format_task_commit_message(task_id, task_title);
        git::commit_all(&resolved.repo_root, &commit_message)?;
        push_if_ahead(&resolved.repo_root)?;
        git::require_clean_repo_ignoring_paths(
            &resolved.repo_root,
            false,
            git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
        )?;
    } else {
        log::info!("Auto git commit/push disabled; leaving repo dirty after queue updates.");
    }
    Ok(())
}

/// Pushes to upstream if the local branch is ahead.
pub(crate) fn push_if_ahead(repo_root: &Path) -> Result<()> {
    match git::is_ahead_of_upstream(repo_root) {
        Ok(ahead) => {
            if !ahead {
                return Ok(());
            }
        }
        Err(GitError::NoUpstream) | Err(GitError::NoUpstreamConfigured) => {
            log::warn!("skipping push (no upstream configured)");
            return Ok(());
        }
        Err(err) => {
            return Err(anyhow!("upstream check failed: {:#}", err));
        }
    }
    if let Err(err) = git::push_upstream(repo_root) {
        bail!(
            "Git push failed: the repository has unpushed commits but the push operation failed. Push manually to sync with upstream. Error: {:#}",
            err
        );
    }
    Ok(())
}

/// Validates LFS configuration and warns about potential issues.
///
/// When `strict` is true, returns an error if LFS filters are misconfigured
/// or if there are files that should be LFS but aren't tracked properly.
pub(crate) fn warn_if_modified_lfs(repo_root: &Path, strict: bool) -> Result<()> {
    match git::has_lfs(repo_root) {
        Ok(true) => {}
        Ok(false) => return Ok(()),
        Err(err) => {
            log::warn!("Git LFS detection failed: {:#}", err);
            return Ok(());
        }
    }

    // Perform comprehensive LFS health check
    let health_report = match git::check_lfs_health(repo_root) {
        Ok(report) => report,
        Err(err) => {
            log::warn!("Git LFS health check failed: {:#}", err);
            return Ok(());
        }
    };

    if !health_report.lfs_initialized {
        return Ok(());
    }

    // Check filter configuration
    if let Some(ref filter_status) = health_report.filter_status
        && !filter_status.is_healthy()
    {
        let issues = filter_status.issues();
        if strict {
            return Err(anyhow!(
                "Git LFS filters misconfigured: {}. Run 'git lfs install' to fix.",
                issues.join("; ")
            ));
        } else {
            log::error!(
                "Git LFS filters misconfigured: {}. Run 'git lfs install' to fix. This may cause data loss if LFS files are committed as pointers!",
                issues.join("; ")
            );
        }
    }

    // Check LFS status for untracked files
    if let Some(ref status_summary) = health_report.status_summary
        && !status_summary.is_clean()
    {
        let issues = status_summary.issue_descriptions();
        if strict {
            return Err(anyhow!("Git LFS issues detected: {}", issues.join("; ")));
        } else {
            for issue in issues {
                log::warn!("LFS issue: {}", issue);
            }
        }
    }

    // Check for pointer file issues
    if !health_report.pointer_issues.is_empty() {
        for issue in &health_report.pointer_issues {
            if strict {
                return Err(anyhow!("LFS pointer issue: {}", issue.description()));
            } else {
                log::warn!("LFS pointer issue: {}", issue.description());
            }
        }
    }

    // Original modified files check
    let status_paths = match git::status_paths(repo_root) {
        Ok(paths) => paths,
        Err(err) => {
            log::warn!("Unable to read git status for LFS warning: {:#}", err);
            return Ok(());
        }
    };

    if status_paths.is_empty() {
        return Ok(());
    }

    let lfs_files = match git::list_lfs_files(repo_root) {
        Ok(files) => files,
        Err(err) => {
            log::warn!("Unable to list LFS files: {:#}", err);
            return Ok(());
        }
    };

    if lfs_files.is_empty() {
        log::warn!(
            "Git LFS detected but no tracked files were listed; review LFS changes manually."
        );
        return Ok(());
    }

    let modified = git::filter_modified_lfs_files(&status_paths, &lfs_files);
    if !modified.is_empty() {
        log::warn!("Modified Git LFS files detected: {}", modified.join(", "));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;

    #[test]
    fn push_if_ahead_skips_when_not_ahead() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        // Create a file to commit (init_repo creates .ralph dir but git needs a file)
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;

        // No upstream configured, so should skip without error
        push_if_ahead(temp.path())?;

        Ok(())
    }

    #[test]
    fn push_if_ahead_errors_on_missing_remote() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        // Create a file to commit (init_repo creates .ralph dir but git needs a file)
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;

        // Set up a remote that doesn't exist
        let remote = TempDir::new()?;
        git_test::git_run(remote.path(), &["init", "--bare"])?;
        let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        )?;
        git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;

        // Now change the remote URL to something that doesn't exist
        let missing_remote = temp.path().join("missing-remote");
        git_test::git_run(
            temp.path(),
            &[
                "remote",
                "set-url",
                "origin",
                missing_remote.to_str().unwrap(),
            ],
        )?;

        // Create a commit so we're ahead
        std::fs::write(temp.path().join("work.txt"), "change")?;
        git_test::commit_all(temp.path(), "work")?;

        // Should error on push failure
        let err = push_if_ahead(temp.path()).unwrap_err();
        assert!(format!("{err:#}").contains("Git push failed"));

        Ok(())
    }
}
