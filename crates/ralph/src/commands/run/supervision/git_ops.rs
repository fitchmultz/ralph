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

use super::PushPolicy;
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
    push_policy: PushPolicy,
) -> Result<()> {
    if git_commit_push_enabled {
        let commit_message = outpututil::format_task_commit_message(task_id, task_title);
        git::commit_all(&resolved.repo_root, &commit_message)?;
        push_if_ahead(&resolved.repo_root, push_policy)?;
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
pub(crate) fn push_if_ahead(repo_root: &Path, push_policy: PushPolicy) -> Result<()> {
    match git::is_ahead_of_upstream(repo_root) {
        Ok(ahead) => {
            if !ahead {
                return Ok(());
            }
            if let Err(err) = git::push_upstream_with_rebase(repo_root) {
                bail!(
                    "Git push failed: the repository has unpushed commits and rebase-aware push failed. Push manually to sync with upstream. Error: {:#}",
                    err
                );
            }
            Ok(())
        }
        Err(GitError::NoUpstream) | Err(GitError::NoUpstreamConfigured) => match push_policy {
            PushPolicy::RequireUpstream => {
                let branch = git::current_branch(repo_root).unwrap_or_else(|_| "HEAD".to_string());
                log::warn!(
                    "skipping push for branch '{}' (no upstream configured). Set upstream with `git push -u origin {}` or run with upstream creation enabled.",
                    branch,
                    branch
                );
                Ok(())
            }
            PushPolicy::AllowCreateUpstream => {
                if let Err(err) = git::push_upstream_with_rebase(repo_root) {
                    bail!(
                        "Git push failed: unable to sync branch without upstream using rebase-aware push. Push manually to sync with upstream. Error: {:#}",
                        err
                    );
                }
                Ok(())
            }
        },
        Err(err) => Err(anyhow!("upstream check failed: {:#}", err)),
    }
}

/// Validates LFS configuration and warns about potential issues.
///
/// When `strict` is true, returns an error if LFS filters are misconfigured,
/// if there are files that should be LFS but aren't tracked properly, or if
/// any git/LFS command fails unexpectedly.
///
/// When `strict` is false, logs warnings for any issues or command failures
/// and returns `Ok(())`.
pub(crate) fn warn_if_modified_lfs(repo_root: &Path, strict: bool) -> Result<()> {
    match git::has_lfs(repo_root) {
        Ok(true) => {}
        Ok(false) => return Ok(()),
        Err(err) => {
            if strict {
                return Err(anyhow!("Git LFS detection failed: {:#}", err));
            }
            log::warn!("Git LFS detection failed: {:#}", err);
            return Ok(());
        }
    }

    // Perform comprehensive LFS health check
    let health_report = match git::check_lfs_health(repo_root) {
        Ok(report) => report,
        Err(err) => {
            if strict {
                return Err(anyhow!("Git LFS health check failed: {:#}", err));
            }
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
            if strict {
                return Err(anyhow!(
                    "Unable to read git status for LFS check: {:#}",
                    err
                ));
            }
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
            if strict {
                return Err(anyhow!("Unable to list LFS files: {:#}", err));
            }
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
        push_if_ahead(temp.path(), PushPolicy::RequireUpstream)?;

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
        let err = push_if_ahead(temp.path(), PushPolicy::RequireUpstream).unwrap_err();
        assert!(format!("{err:#}").contains("Git push failed"));

        Ok(())
    }

    #[test]
    fn push_if_ahead_creates_upstream_when_allowed() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;

        let remote = TempDir::new()?;
        git_test::git_run(remote.path(), &["init", "--bare"])?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        )?;

        std::fs::write(temp.path().join("work.txt"), "change")?;
        git_test::commit_all(temp.path(), "work")?;

        push_if_ahead(temp.path(), PushPolicy::AllowCreateUpstream)?;

        let upstream = git_test::git_output(
            temp.path(),
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )?;
        assert!(upstream.starts_with("origin/"));

        Ok(())
    }

    #[test]
    fn push_if_ahead_allow_create_handles_existing_remote_branch_without_local_upstream()
    -> Result<()> {
        let remote = TempDir::new()?;
        git_test::init_bare_repo(remote.path())?;

        let seed = TempDir::new()?;
        git_test::init_repo(seed.path())?;
        git_test::add_remote(seed.path(), "origin", remote.path())?;
        std::fs::write(seed.path().join("base.txt"), "base\n")?;
        git_test::commit_all(seed.path(), "init")?;
        git_test::git_run(seed.path(), &["push", "-u", "origin", "HEAD"])?;
        git_test::git_run(seed.path(), &["checkout", "-b", "ralph/RQ-0940"])?;
        std::fs::write(seed.path().join("task.txt"), "remote-only\n")?;
        git_test::commit_all(seed.path(), "remote task")?;
        git_test::git_run(seed.path(), &["push", "-u", "origin", "ralph/RQ-0940"])?;

        let local = TempDir::new()?;
        git_test::clone_repo(remote.path(), local.path())?;
        git_test::configure_user(local.path())?;
        git_test::git_run(
            local.path(),
            &[
                "checkout",
                "--no-track",
                "-b",
                "ralph/RQ-0940",
                "origin/main",
            ],
        )?;

        // Should not fail with non-fast-forward; should attach upstream and continue.
        push_if_ahead(local.path(), PushPolicy::AllowCreateUpstream)?;

        let upstream = git_test::git_output(
            local.path(),
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )?;
        assert_eq!(upstream, "origin/ralph/RQ-0940");

        Ok(())
    }

    #[test]
    fn push_if_ahead_recovers_from_non_fast_forward() -> Result<()> {
        let remote = TempDir::new()?;
        git_test::init_bare_repo(remote.path())?;

        let repo_a = TempDir::new()?;
        git_test::init_repo(repo_a.path())?;
        git_test::add_remote(repo_a.path(), "origin", remote.path())?;

        std::fs::write(repo_a.path().join("base.txt"), "base\n")?;
        git_test::commit_all(repo_a.path(), "init")?;
        git_test::git_run(repo_a.path(), &["push", "-u", "origin", "HEAD"])?;

        let repo_b = TempDir::new()?;
        git_test::clone_repo(remote.path(), repo_b.path())?;
        git_test::configure_user(repo_b.path())?;
        std::fs::write(repo_b.path().join("remote.txt"), "remote\n")?;
        git_test::commit_all(repo_b.path(), "remote update")?;
        git_test::git_run(repo_b.path(), &["push"])?;

        std::fs::write(repo_a.path().join("local.txt"), "local\n")?;
        git_test::commit_all(repo_a.path(), "local update")?;

        // Should succeed by rebasing local commit onto remote and retrying push.
        push_if_ahead(repo_a.path(), PushPolicy::RequireUpstream)?;

        let verify = TempDir::new()?;
        git_test::clone_repo(remote.path(), verify.path())?;
        let history =
            git_test::git_output(verify.path(), &["log", "--oneline", "--max-count", "4"])?;
        assert!(
            history.contains("local update"),
            "expected rebased local commit in remote history: {}",
            history
        );
        assert!(
            history.contains("remote update"),
            "expected remote commit preserved in history: {}",
            history
        );

        Ok(())
    }

    #[test]
    fn warn_if_modified_lfs_strict_errors_when_lfs_detected_but_git_config_fails() {
        let temp = TempDir::new().expect("tempdir");
        // Create a valid git repo
        git_test::init_repo(temp.path()).expect("init repo");
        // Create .gitattributes with LFS filter
        std::fs::write(temp.path().join(".gitattributes"), "*.bin filter=lfs\n")
            .expect("write gitattributes");
        // Create a fake .git/lfs directory to trigger LFS detection
        std::fs::create_dir_all(temp.path().join(".git/lfs")).expect("create lfs dir");

        // Break git by corrupting .git/config
        std::fs::write(temp.path().join(".git/config"), "not a valid config")
            .expect("write invalid config");

        let err = warn_if_modified_lfs(temp.path(), true).unwrap_err();
        let msg = format!("{err:#}");
        // Should fail because git config commands fail with invalid config
        assert!(
            msg.to_lowercase().contains("git") || msg.to_lowercase().contains("lfs"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn warn_if_modified_lfs_non_strict_warns_and_continues_on_errors() -> Result<()> {
        let temp = TempDir::new()?;
        // Create a valid git repo
        git_test::init_repo(temp.path())?;
        // Create .gitattributes with LFS filter
        std::fs::write(temp.path().join(".gitattributes"), "*.bin filter=lfs\n")?;
        // Create a fake .git/lfs directory to trigger LFS detection
        std::fs::create_dir_all(temp.path().join(".git/lfs"))?;

        // Break git by corrupting .git/config
        std::fs::write(temp.path().join(".git/config"), "not a valid config")?;

        // Should return Ok(()) in non-strict mode even though git commands will fail
        // because `strict=false` is warn-and-continue.
        warn_if_modified_lfs(temp.path(), false)?;
        Ok(())
    }
}
