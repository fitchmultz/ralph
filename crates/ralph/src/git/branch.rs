//! Git branch helpers for resolving the current branch name.
//!
//! Responsibilities:
//! - Determine the current branch name for the repository.
//! - Fail fast on detached HEAD states to avoid ambiguous base branches.
//!
//! Not handled here:
//! - Branch creation or deletion (see `git/worktree.rs`).
//! - Push/pull operations (see `git/commit.rs`).
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

#[cfg(test)]
mod tests {
    use super::current_branch;
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
}
