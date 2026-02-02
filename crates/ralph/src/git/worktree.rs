//! Git worktree helpers for parallel task isolation.
//!
//! Responsibilities:
//! - Create and remove git worktrees for parallel task execution.
//! - Compute the worktree root path using resolved configuration.
//!
//! Not handled here:
//! - Task selection or worker orchestration (see `commands::run::parallel`).
//! - PR creation or merge operations (see `git/pr.rs`).
//!
//! Invariants/assumptions:
//! - `git` is available and the repo root is valid.
//! - Worktree paths are unique per task ID.

use crate::contracts::Config;
use crate::git::error::git_base_command;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct WorktreeSpec {
    pub task_id: String,
    pub path: PathBuf,
    pub branch: String,
}

pub(crate) fn worktree_root(repo_root: &Path, cfg: &Config) -> PathBuf {
    let root = cfg
        .parallel
        .worktree_root
        .clone()
        .unwrap_or_else(|| PathBuf::from(".ralph/worktrees/parallel"));
    if root.is_absolute() {
        root
    } else {
        repo_root.join(root)
    }
}

pub(crate) fn create_worktree_at(
    repo_root: &Path,
    worktree_root: &Path,
    task_id: &str,
    base_branch: &str,
    branch_prefix: &str,
) -> Result<WorktreeSpec> {
    let trimmed_id = task_id.trim();
    if trimmed_id.is_empty() {
        bail!("worktree task_id must be non-empty");
    }

    let branch = format!("{}{}", branch_prefix, trimmed_id);
    let path = worktree_root.join(trimmed_id);

    fs::create_dir_all(worktree_root)
        .with_context(|| format!("create worktree root directory {}", worktree_root.display()))?;

    let output = git_base_command(repo_root)
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg(&branch)
        .arg(&path)
        .arg(base_branch)
        .output()
        .with_context(|| format!("run git worktree add in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git worktree add failed for task {}: {}",
            trimmed_id,
            stderr.trim()
        );
    }

    Ok(WorktreeSpec {
        task_id: trimmed_id.to_string(),
        path,
        branch,
    })
}

pub(crate) fn remove_worktree(repo_root: &Path, spec: &WorktreeSpec, force: bool) -> Result<()> {
    let mut cmd = git_base_command(repo_root);
    cmd.arg("worktree").arg("remove");
    if force {
        cmd.arg("--force");
    }
    cmd.arg(&spec.path);
    let output = cmd
        .output()
        .with_context(|| format!("run git worktree remove in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git worktree remove failed for task {}: {}",
            spec.task_id,
            stderr.trim()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Config, ParallelConfig};
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;

    #[test]
    fn worktree_root_uses_repo_root_for_relative_path() {
        let cfg = Config {
            parallel: ParallelConfig {
                worktree_root: Some(PathBuf::from(".ralph/worktrees/custom")),
                ..ParallelConfig::default()
            },
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = worktree_root(&repo_root, &cfg);
        assert_eq!(
            root,
            PathBuf::from("/tmp/ralph-test/.ralph/worktrees/custom")
        );
    }

    #[test]
    fn worktree_root_accepts_absolute_path() {
        let cfg = Config {
            parallel: ParallelConfig {
                worktree_root: Some(PathBuf::from("/tmp/ralph-worktrees")),
                ..ParallelConfig::default()
            },
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = worktree_root(&repo_root, &cfg);
        assert_eq!(root, PathBuf::from("/tmp/ralph-worktrees"));
    }

    #[test]
    fn create_and_remove_worktree_round_trips() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let root = temp.path().join(".ralph/worktrees/parallel");

        let spec = create_worktree_at(temp.path(), &root, "RQ-0001", &base_branch, "ralph/")?;

        assert!(spec.path.exists(), "worktree path should exist");

        remove_worktree(temp.path(), &spec, true)?;
        Ok(())
    }
}
