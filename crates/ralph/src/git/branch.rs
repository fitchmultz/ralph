//! Git branch helpers for resolving the current branch name.
//!
//! Responsibilities:
//! - Determine the current branch name for the repository.
//! - Fail fast on detached HEAD states to avoid ambiguous base branches.
//! - Fast-forward the local base branch to `origin/<branch>` when required.
//!
//! Not handled here:
//! - Branch creation or deletion (see `git/workspace.rs`).
//! - Push operations (see `git/commit.rs`).
//!
//! Invariants/assumptions:
//! - Caller expects a named branch (not detached HEAD).
//! - Git is available and the repo root is valid.

use crate::git::error::git_base_command;
use anyhow::{Context, Result, bail};
use std::path::Path;

pub(crate) fn current_branch(repo_root: &Path) -> Result<String> {
    let output = git_base_command(repo_root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .with_context(|| {
            format!(
                "run git rev-parse --abbrev-ref HEAD in {}",
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Failed to determine current branch: git rev-parse error: {}",
            stderr.trim()
        );
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        bail!("Failed to determine current branch: empty branch name.");
    }

    if branch == "HEAD" {
        bail!("Parallel run requires a named branch (detached HEAD detected).");
    }

    Ok(branch)
}

#[allow(dead_code)]
pub(crate) fn fast_forward_branch_to_origin(repo_root: &Path, branch: &str) -> Result<()> {
    let branch = branch.trim();
    if branch.is_empty() {
        bail!("Cannot fast-forward: branch name is empty.");
    }

    let checkout_output = git_base_command(repo_root)
        .args(["checkout", branch])
        .output()
        .with_context(|| format!("run git checkout {} in {}", branch, repo_root.display()))?;
    if !checkout_output.status.success() {
        let stderr = String::from_utf8_lossy(&checkout_output.stderr);
        bail!(
            "Failed to check out branch {} before fast-forward: {}",
            branch,
            stderr.trim()
        );
    }

    let fetch_output = git_base_command(repo_root)
        .args(["fetch", "origin", "--prune"])
        .output()
        .with_context(|| format!("run git fetch origin --prune in {}", repo_root.display()))?;
    if !fetch_output.status.success() {
        let stderr = String::from_utf8_lossy(&fetch_output.stderr);
        bail!(
            "Failed to fetch origin before fast-forwarding {}: {}",
            branch,
            stderr.trim()
        );
    }

    let remote_ref = format!("origin/{}", branch);
    let merge_output = git_base_command(repo_root)
        .args(["merge", "--ff-only", &remote_ref])
        .output()
        .with_context(|| {
            format!(
                "run git merge --ff-only {} in {}",
                remote_ref,
                repo_root.display()
            )
        })?;
    if !merge_output.status.success() {
        let stderr = String::from_utf8_lossy(&merge_output.stderr);
        bail!(
            "Failed to fast-forward branch {} to {}: {}",
            branch,
            remote_ref,
            stderr.trim()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{current_branch, fast_forward_branch_to_origin};
    use crate::testsupport::git as git_test;
    use anyhow::Result;
    use tempfile::TempDir;

    #[test]
    fn current_branch_returns_branch_name() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        let expected = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let branch = current_branch(temp.path())?;
        assert_eq!(branch, expected);
        Ok(())
    }

    #[test]
    fn current_branch_errors_on_detached_head() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(temp.path(), &["checkout", "--detach", "HEAD"])?;
        let err = current_branch(temp.path()).unwrap_err();
        assert!(err.to_string().contains("detached HEAD"));
        Ok(())
    }

    #[test]
    fn fast_forward_branch_to_origin_updates_local_branch() -> Result<()> {
        let temp = TempDir::new()?;
        let remote = temp.path().join("remote.git");
        std::fs::create_dir_all(&remote)?;
        git_test::init_bare_repo(&remote)?;

        let seed = temp.path().join("seed");
        std::fs::create_dir_all(&seed)?;
        git_test::init_repo(&seed)?;
        std::fs::write(seed.join("seed.txt"), "v1")?;
        git_test::commit_all(&seed, "seed init")?;
        let branch = git_test::git_output(&seed, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        git_test::add_remote(&seed, "origin", &remote)?;
        git_test::push_branch(&seed, &branch)?;
        git_test::git_run(
            &remote,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{}", branch)],
        )?;

        let local = temp.path().join("local");
        git_test::clone_repo(&remote, &local)?;
        git_test::configure_user(&local)?;

        let upstream = temp.path().join("upstream");
        git_test::clone_repo(&remote, &upstream)?;
        git_test::configure_user(&upstream)?;
        std::fs::write(upstream.join("seed.txt"), "v2")?;
        git_test::commit_all(&upstream, "remote ahead")?;
        git_test::push_branch(&upstream, &branch)?;

        let old_head = git_test::git_output(&local, &["rev-parse", "HEAD"])?;
        fast_forward_branch_to_origin(&local, &branch)?;
        let new_head = git_test::git_output(&local, &["rev-parse", "HEAD"])?;
        let remote_head =
            git_test::git_output(&local, &["rev-parse", &format!("origin/{}", branch)])?;

        assert_ne!(old_head, new_head);
        assert_eq!(new_head, remote_head);
        Ok(())
    }

    #[test]
    fn fast_forward_branch_to_origin_errors_on_divergence() -> Result<()> {
        let temp = TempDir::new()?;
        let remote = temp.path().join("remote.git");
        std::fs::create_dir_all(&remote)?;
        git_test::init_bare_repo(&remote)?;

        let seed = temp.path().join("seed");
        std::fs::create_dir_all(&seed)?;
        git_test::init_repo(&seed)?;
        std::fs::write(seed.join("seed.txt"), "v1")?;
        git_test::commit_all(&seed, "seed init")?;
        let branch = git_test::git_output(&seed, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        git_test::add_remote(&seed, "origin", &remote)?;
        git_test::push_branch(&seed, &branch)?;
        git_test::git_run(
            &remote,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{}", branch)],
        )?;

        let local = temp.path().join("local");
        git_test::clone_repo(&remote, &local)?;
        git_test::configure_user(&local)?;

        let upstream = temp.path().join("upstream");
        git_test::clone_repo(&remote, &upstream)?;
        git_test::configure_user(&upstream)?;

        std::fs::write(local.join("local.txt"), "local-only")?;
        git_test::commit_all(&local, "local ahead")?;

        std::fs::write(upstream.join("remote.txt"), "remote-only")?;
        git_test::commit_all(&upstream, "remote ahead")?;
        git_test::push_branch(&upstream, &branch)?;

        let err = fast_forward_branch_to_origin(&local, &branch).unwrap_err();
        assert!(
            err.to_string().contains("fast-forward"),
            "unexpected error: {err}"
        );
        Ok(())
    }
}
