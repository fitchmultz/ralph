//! Git commit and push operations.
//!
//! This module provides functions for creating commits, restoring tracked paths,
//! and pushing to upstream remotes. It handles error classification and provides
//! clear feedback on failures.
//!
//! # Invariants
//! - Upstream must be configured before pushing
//! - Empty commit messages are rejected
//! - No-changes commits are rejected
//! - Path restores only operate on tracked files under the repo root
//!
//! # What this does NOT handle
//! - Status checking (see git/status.rs)
//! - LFS validation (see git/lfs.rs)
//! - Repository cleanliness (see git/clean.rs)

use crate::git::current_branch;
use crate::git::error::{GitError, classify_push_error, git_base_command, git_run};
use anyhow::Context;
use std::path::{Path, PathBuf};

use crate::git::status::status_porcelain;

/// Revert uncommitted changes, restoring the working tree to current HEAD.
///
/// This discards ONLY uncommitted changes. It does NOT reset to a pre-run SHA.
pub fn revert_uncommitted(repo_root: &Path) -> Result<(), GitError> {
    // Revert tracked changes in both index and working tree.
    // Prefer `git restore` (modern); fall back to older `git checkout` syntax.
    if git_run(repo_root, &["restore", "--staged", "--worktree", "."]).is_err() {
        // Older git fallback.
        git_run(repo_root, &["checkout", "--", "."]).context("fallback git checkout -- .")?;
        // Ensure staged changes are cleared too.
        git_run(repo_root, &["reset", "--quiet", "HEAD"]).context("git reset --quiet HEAD")?;
    }

    // Remove untracked files/directories created during the run.
    git_run(
        repo_root,
        &[
            "clean",
            "-fd",
            "-e",
            ".env",
            "-e",
            ".env.*",
            "-e",
            ".ralph/cache/completions",
        ],
    )
    .context("git clean -fd -e .env*")?;
    Ok(())
}

/// Create a commit with all changes.
///
/// Stages everything and creates a single commit with the given message.
/// Returns an error if the message is empty or there are no changes to commit.
pub fn commit_all(repo_root: &Path, message: &str) -> Result<(), GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::EmptyCommitMessage);
    }

    git_run(repo_root, &["add", "-A"]).context("git add -A")?;
    let status = status_porcelain(repo_root)?;
    if status.trim().is_empty() {
        return Err(GitError::NoChangesToCommit);
    }

    git_run(repo_root, &["commit", "-m", message]).context("git commit")?;
    Ok(())
}

/// Force-add existing paths, even if they are ignored.
///
/// Paths must be under the repo root; missing or outside paths are skipped.
pub fn add_paths_force(repo_root: &Path, paths: &[PathBuf]) -> Result<(), GitError> {
    if paths.is_empty() {
        return Ok(());
    }

    let mut rel_paths: Vec<String> = Vec::new();
    for path in paths {
        if !path.exists() {
            continue;
        }
        let rel = match path.strip_prefix(repo_root) {
            Ok(rel) => rel,
            Err(_) => {
                log::debug!(
                    "Skipping force-add for path outside repo root: {}",
                    path.display()
                );
                continue;
            }
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        rel_paths.push(rel.to_string_lossy().to_string());
    }

    if rel_paths.is_empty() {
        return Ok(());
    }

    let mut add_args: Vec<String> = vec!["add".to_string(), "-f".to_string(), "--".to_string()];
    add_args.extend(rel_paths.iter().cloned());
    let add_refs: Vec<&str> = add_args.iter().map(|s| s.as_str()).collect();
    git_run(repo_root, &add_refs).context("git add -f -- <paths>")?;
    Ok(())
}

/// Restore tracked paths to the current HEAD (index + working tree).
///
/// Paths must be under the repo root; untracked paths are skipped.
pub fn restore_tracked_paths_to_head(repo_root: &Path, paths: &[PathBuf]) -> Result<(), GitError> {
    if paths.is_empty() {
        return Ok(());
    }

    let mut rel_paths: Vec<String> = Vec::new();
    for path in paths {
        let rel = match path.strip_prefix(repo_root) {
            Ok(rel) => rel,
            Err(_) => {
                log::debug!(
                    "Skipping restore for path outside repo root: {}",
                    path.display()
                );
                continue;
            }
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let rel_str = rel.to_string_lossy().to_string();
        if is_tracked_path(repo_root, &rel_str)? {
            rel_paths.push(rel_str);
        } else {
            log::debug!("Skipping restore for untracked path: {}", rel.display());
        }
    }

    if rel_paths.is_empty() {
        return Ok(());
    }

    let mut restore_args: Vec<String> = vec![
        "restore".to_string(),
        "--staged".to_string(),
        "--worktree".to_string(),
        "--".to_string(),
    ];
    restore_args.extend(rel_paths.iter().cloned());
    let restore_refs: Vec<&str> = restore_args.iter().map(|s| s.as_str()).collect();
    if git_run(repo_root, &restore_refs).is_err() {
        let mut checkout_args: Vec<String> = vec!["checkout".to_string(), "--".to_string()];
        checkout_args.extend(rel_paths.iter().cloned());
        let checkout_refs: Vec<&str> = checkout_args.iter().map(|s| s.as_str()).collect();
        git_run(repo_root, &checkout_refs).context("fallback git checkout -- <paths>")?;

        let mut reset_args: Vec<String> = vec![
            "reset".to_string(),
            "--quiet".to_string(),
            "HEAD".to_string(),
            "--".to_string(),
        ];
        reset_args.extend(rel_paths.iter().cloned());
        let reset_refs: Vec<&str> = reset_args.iter().map(|s| s.as_str()).collect();
        git_run(repo_root, &reset_refs).context("git reset --quiet HEAD -- <paths>")?;
    }

    Ok(())
}

fn is_tracked_path(repo_root: &Path, rel_path: &str) -> Result<bool, GitError> {
    let output = git_base_command(repo_root)
        .args(["ls-files", "--error-unmatch", "--", rel_path])
        .output()
        .with_context(|| {
            format!(
                "run git ls-files --error-unmatch for {} in {}",
                rel_path,
                repo_root.display()
            )
        })?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    if stderr.contains("pathspec") || stderr.contains("did not match any file") {
        return Ok(false);
    }

    Err(GitError::CommandFailed {
        args: format!("ls-files --error-unmatch -- {}", rel_path),
        code: output.status.code(),
        stderr: stderr.trim().to_string(),
    })
}

/// Get the configured upstream for the current branch.
///
/// Returns the upstream reference (e.g. "origin/main") or an error if not configured.
pub fn upstream_ref(repo_root: &Path) -> Result<String, GitError> {
    let output = git_base_command(repo_root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("--symbolic-full-name")
        .arg("@{u}")
        .output()
        .with_context(|| {
            format!(
                "run git rev-parse --abbrev-ref --symbolic-full-name @{{u}} in {}",
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(classify_push_error(&stderr));
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        return Err(GitError::NoUpstreamConfigured);
    }
    Ok(value)
}

/// Check if HEAD is ahead of the configured upstream.
///
/// Returns true if there are local commits that haven't been pushed.
pub fn is_ahead_of_upstream(repo_root: &Path) -> Result<bool, GitError> {
    let upstream = upstream_ref(repo_root)?;
    let range = format!("{upstream}...HEAD");
    let output = git_base_command(repo_root)
        .arg("rev-list")
        .arg("--left-right")
        .arg("--count")
        .arg(range)
        .output()
        .with_context(|| {
            format!(
                "run git rev-list --left-right --count in {}",
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(GitError::CommandFailed {
            args: "rev-list --left-right --count".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    let counts = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = counts.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(GitError::UnexpectedRevListOutput(counts.trim().to_string()));
    }

    let ahead: u32 = parts[1].parse().context("parse ahead count")?;
    Ok(ahead > 0)
}

/// Push HEAD to the configured upstream.
///
/// Returns an error if push fails due to authentication, missing upstream,
/// or other git errors.
pub fn push_upstream(repo_root: &Path) -> Result<(), GitError> {
    let output = git_base_command(repo_root)
        .arg("push")
        .output()
        .with_context(|| format!("run git push in {}", repo_root.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(classify_push_error(&stderr))
}

/// Push HEAD to origin and create upstream tracking.
///
/// Intended for new branches that do not have an upstream configured yet.
pub fn push_upstream_allow_create(repo_root: &Path) -> Result<(), GitError> {
    let output = git_base_command(repo_root)
        .arg("push")
        .arg("-u")
        .arg("origin")
        .arg("HEAD")
        .output()
        .with_context(|| format!("run git push -u origin HEAD in {}", repo_root.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(classify_push_error(&stderr))
}

fn is_non_fast_forward_error(err: &GitError) -> bool {
    let GitError::PushFailed(detail) = err else {
        return false;
    };
    let lower = detail.to_lowercase();
    lower.contains("non-fast-forward")
        || lower.contains("fetch first")
        || lower.contains("rejected")
}

fn rebase_onto(repo_root: &Path, upstream: &str) -> Result<(), GitError> {
    git_run(repo_root, &["fetch", "origin", "--prune"])?;
    git_run(repo_root, &["rebase", upstream])?;
    Ok(())
}

/// Push HEAD to upstream, rebasing on non-fast-forward rejections.
///
/// If the branch has no upstream yet, this will create one via `git push -u origin HEAD`.
/// When the push is rejected because the remote has new commits, this will:
/// - `git fetch origin --prune`
/// - `git rebase <upstream>`
/// - retry the push once
pub fn push_upstream_with_rebase(repo_root: &Path) -> Result<(), GitError> {
    let ahead = match is_ahead_of_upstream(repo_root) {
        Ok(ahead) => ahead,
        Err(GitError::NoUpstream) | Err(GitError::NoUpstreamConfigured) => true,
        Err(err) => return Err(err),
    };

    if !ahead {
        return Ok(());
    }

    let push_result = match push_upstream(repo_root) {
        Ok(()) => return Ok(()),
        Err(GitError::NoUpstream) | Err(GitError::NoUpstreamConfigured) => {
            push_upstream_allow_create(repo_root)
        }
        Err(err) => Err(err),
    };

    match push_result {
        Ok(()) => Ok(()),
        Err(err) if is_non_fast_forward_error(&err) => {
            let upstream = match upstream_ref(repo_root) {
                Ok(upstream) => upstream,
                Err(_) => {
                    let branch = current_branch(repo_root).map_err(GitError::Other)?;
                    format!("origin/{}", branch)
                }
            };
            rebase_onto(repo_root, &upstream)?;
            if upstream_ref(repo_root).is_ok() {
                push_upstream(repo_root)
            } else {
                push_upstream_allow_create(repo_root)
            }
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;

    #[test]
    fn push_upstream_with_rebase_recovers_from_non_fast_forward() -> anyhow::Result<()> {
        let remote = TempDir::new()?;
        git_test::init_bare_repo(remote.path())?;

        let repo_a = TempDir::new()?;
        git_test::init_repo(repo_a.path())?;
        git_test::add_remote(repo_a.path(), "origin", remote.path())?;

        std::fs::write(repo_a.path().join("base.txt"), "init\n")?;
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

        push_upstream_with_rebase(repo_a.path())?;

        let counts = git_test::git_output(
            repo_a.path(),
            &["rev-list", "--left-right", "--count", "@{u}...HEAD"],
        )?;
        let parts: Vec<&str> = counts.split_whitespace().collect();
        assert_eq!(parts, vec!["0", "0"]);

        Ok(())
    }
}
