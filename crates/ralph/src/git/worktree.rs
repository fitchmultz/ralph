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
        .unwrap_or_else(|| default_worktree_root(repo_root));
    if root.is_absolute() {
        root
    } else {
        repo_root.join(root)
    }
}

fn default_worktree_root(repo_root: &Path) -> PathBuf {
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repo");
    let parent = repo_root.parent().unwrap_or(repo_root);
    parent.join(".worktrees").join(repo_name).join("parallel")
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

    if let Err(err) = prune_worktrees(repo_root) {
        log::warn!("Failed to prune worktrees: {:#}", err);
    }

    if let Some(existing_path) = existing_worktree_for_branch(repo_root, &branch)?
        && existing_path.exists()
    {
        if !existing_path.starts_with(worktree_root) {
            log::warn!(
                "Existing worktree for {} at {} is outside configured root {}; removing.",
                branch,
                existing_path.display(),
                worktree_root.display()
            );
            remove_worktree(
                repo_root,
                &WorktreeSpec {
                    task_id: trimmed_id.to_string(),
                    path: existing_path.clone(),
                    branch: branch.clone(),
                },
                true,
            )?;
        } else {
            ensure_clean_worktree(&existing_path, base_branch)?;
            log::info!(
                "Reusing existing worktree for {} at {}",
                branch,
                existing_path.display()
            );
            return Ok(WorktreeSpec {
                task_id: trimmed_id.to_string(),
                path: existing_path,
                branch,
            });
        }
    }

    if path.exists() {
        bail!(
            "worktree path already exists for task {}: {}",
            trimmed_id,
            path.display()
        );
    }

    let output = if branch_exists(repo_root, &branch)? {
        reset_branch_to_base(repo_root, &branch, base_branch)?;
        git_base_command(repo_root)
            .arg("worktree")
            .arg("add")
            .arg(&path)
            .arg(&branch)
            .output()
            .with_context(|| format!("run git worktree add in {}", repo_root.display()))?
    } else {
        git_base_command(repo_root)
            .arg("worktree")
            .arg("add")
            .arg("-b")
            .arg(&branch)
            .arg(&path)
            .arg(base_branch)
            .output()
            .with_context(|| format!("run git worktree add in {}", repo_root.display()))?
    };

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

fn branch_exists(repo_root: &Path, branch: &str) -> Result<bool> {
    let output = git_base_command(repo_root)
        .arg("show-ref")
        .arg("--verify")
        .arg("--quiet")
        .arg(format!("refs/heads/{}", branch))
        .output()
        .with_context(|| format!("run git show-ref in {}", repo_root.display()))?;

    if output.status.success() {
        return Ok(true);
    }
    match output.status.code() {
        Some(1) => Ok(false),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git show-ref failed: {}", stderr.trim())
        }
    }
}

fn prune_worktrees(repo_root: &Path) -> Result<()> {
    let output = git_base_command(repo_root)
        .arg("worktree")
        .arg("prune")
        .output()
        .with_context(|| format!("run git worktree prune in {}", repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree prune failed: {}", stderr.trim());
    }
    Ok(())
}

fn existing_worktree_for_branch(repo_root: &Path, branch: &str) -> Result<Option<PathBuf>> {
    let output = git_base_command(repo_root)
        .arg("worktree")
        .arg("list")
        .arg("--porcelain")
        .output()
        .with_context(|| format!("run git worktree list in {}", repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree list failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;

    let maybe_match = |path: Option<PathBuf>, branch_name: Option<String>| {
        if let (Some(path), Some(branch_name)) = (path, branch_name)
            && branch_name == branch
        {
            return Some(path);
        }
        None
    };

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(found) = maybe_match(current_path.take(), current_branch.take()) {
                return Ok(Some(found));
            }
            current_path = Some(PathBuf::from(rest.trim()));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            let value = rest.trim();
            if value != "(detached)" {
                let name = value
                    .strip_prefix("refs/heads/")
                    .unwrap_or(value)
                    .to_string();
                current_branch = Some(name);
            }
        }
    }

    if let Some(found) = maybe_match(current_path, current_branch) {
        return Ok(Some(found));
    }

    Ok(None)
}

fn reset_branch_to_base(repo_root: &Path, branch: &str, base_branch: &str) -> Result<()> {
    let output = git_base_command(repo_root)
        .arg("branch")
        .arg("-f")
        .arg(branch)
        .arg(base_branch)
        .output()
        .with_context(|| format!("run git branch -f in {}", repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch -f failed: {}", stderr.trim());
    }
    Ok(())
}

fn ensure_clean_worktree(worktree_path: &Path, base_branch: &str) -> Result<()> {
    let output = git_base_command(worktree_path)
        .arg("reset")
        .arg("--hard")
        .arg(base_branch)
        .output()
        .with_context(|| format!("run git reset in {}", worktree_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git reset --hard failed: {}", stderr.trim());
    }

    let output = git_base_command(worktree_path)
        .arg("clean")
        .arg("-fd")
        .output()
        .with_context(|| format!("run git clean in {}", worktree_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git clean failed: {}", stderr.trim());
    }
    Ok(())
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
    fn worktree_root_defaults_outside_repo() {
        let cfg = Config {
            parallel: ParallelConfig::default(),
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = worktree_root(&repo_root, &cfg);
        assert_eq!(root, PathBuf::from("/tmp/.worktrees/ralph-test/parallel"));
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

    #[test]
    fn create_worktree_reuses_existing_branch_worktree() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let root = temp.path().join(".worktrees");

        let first = create_worktree_at(temp.path(), &root, "RQ-0001", &base_branch, "ralph/")?;
        std::fs::write(first.path.join("dirty.txt"), "dirty")?;
        let second = create_worktree_at(temp.path(), &root, "RQ-0001", &base_branch, "ralph/")?;

        assert_eq!(first.path, second.path);
        assert!(second.path.exists());
        assert!(!second.path.join("dirty.txt").exists());

        remove_worktree(temp.path(), &first, true)?;
        Ok(())
    }

    #[test]
    fn create_worktree_with_existing_branch() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        git_test::git_run(temp.path(), &["branch", "ralph/RQ-0002"])?;
        let root = temp.path().join(".worktrees");

        let spec = create_worktree_at(temp.path(), &root, "RQ-0002", &base_branch, "ralph/")?;

        assert!(spec.path.exists());

        remove_worktree(temp.path(), &spec, true)?;
        Ok(())
    }
}
