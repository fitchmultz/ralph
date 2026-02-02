//! Git workspace helpers for parallel task isolation (clone-based).
//!
//! Responsibilities:
//! - Create and remove isolated git workspaces for parallel task execution.
//! - Compute the workspace root path using resolved configuration.
//! - Ensure clones are pushable by resolving the real origin remote.
//!
//! Not handled here:
//! - Task selection or worker orchestration (see `commands::run::parallel`).
//! - PR creation or merge operations (see `git::pr`).
//! - Merge conflict resolution logic (see `commands::run::parallel::merge_runner`).
//!
//! Invariants/assumptions:
//! - `git` is available and the repo root is valid.
//! - Workspace paths are unique per task ID.
//! - Clones must have a pushable `origin` remote.

use crate::contracts::Config;
use crate::git::error::git_base_command;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceSpec {
    pub task_id: String,
    pub path: PathBuf,
    pub branch: String,
}

pub(crate) fn workspace_root(repo_root: &Path, cfg: &Config) -> PathBuf {
    let root = cfg
        .parallel
        .workspace_root
        .clone()
        .unwrap_or_else(|| default_workspace_root(repo_root));
    if root.is_absolute() {
        root
    } else {
        repo_root.join(root)
    }
}

fn default_workspace_root(repo_root: &Path) -> PathBuf {
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repo");
    let parent = repo_root.parent().unwrap_or(repo_root);
    parent.join(".workspaces").join(repo_name).join("parallel")
}

pub(crate) fn create_workspace_at(
    repo_root: &Path,
    workspace_root: &Path,
    task_id: &str,
    base_branch: &str,
    branch_prefix: &str,
) -> Result<WorkspaceSpec> {
    let trimmed_id = task_id.trim();
    if trimmed_id.is_empty() {
        bail!("workspace task_id must be non-empty");
    }

    let branch = format!("{}{}", branch_prefix, trimmed_id);
    let path = workspace_root.join(trimmed_id);

    fs::create_dir_all(workspace_root).with_context(|| {
        format!(
            "create workspace root directory {}",
            workspace_root.display()
        )
    })?;

    let (fetch_url, push_url) = origin_urls(repo_root)?;
    if path.exists() {
        if !path.join(".git").exists() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("remove non-git workspace {}", path.display()))?;
            clone_repo_from_local(repo_root, &path)?;
        }
    } else {
        clone_repo_from_local(repo_root, &path)?;
    }

    retarget_origin(&path, &fetch_url, &push_url)?;
    // Fetch is best-effort: local clone already has refs, and remote may not be reachable in tests.
    let _ = fetch_origin(&path);
    let base_ref = resolve_base_ref(&path, base_branch)?;
    checkout_branch_from_base(&path, &branch, &base_ref)?;
    hard_reset_and_clean(&path, &base_ref)?;

    Ok(WorkspaceSpec {
        task_id: trimmed_id.to_string(),
        path,
        branch,
    })
}

/// Ensures a workspace exists and is properly configured for the given branch.
///
/// If the workspace exists and is a valid git clone (`.git` exists), it is reused.
/// If the workspace exists but is invalid (not a directory / missing `.git`), it is deleted.
/// If missing, it is cloned from `repo_root` (local clone).
///
/// After ensuring the workspace exists:
/// - Retarget `origin` fetch/push URLs to match `repo_root`'s origin (pushable required).
/// - Fetch `origin --prune`.
/// - Checkout/reset branch to remote: `checkout -B <branch> origin/<branch>`.
/// - Hard reset + clean to ensure deterministic working tree.
pub(crate) fn ensure_workspace_exists(
    repo_root: &Path,
    workspace_path: &Path,
    branch: &str,
) -> Result<()> {
    // Validate or create workspace
    if workspace_path.exists() {
        if !workspace_path.join(".git").exists() {
            fs::remove_dir_all(workspace_path).with_context(|| {
                format!(
                    "remove invalid workspace (missing .git) {}",
                    workspace_path.display()
                )
            })?;
            clone_repo_from_local(repo_root, workspace_path)?;
        }
    } else {
        fs::create_dir_all(workspace_path.parent().unwrap_or(workspace_path)).with_context(
            || {
                format!(
                    "create workspace parent directory {}",
                    workspace_path.display()
                )
            },
        )?;
        clone_repo_from_local(repo_root, workspace_path)?;
    }

    // Retarget origin to be pushable
    let (fetch_url, push_url) = origin_urls(repo_root)?;
    retarget_origin(workspace_path, &fetch_url, &push_url)?;

    // Fetch origin --prune (best-effort)
    let _ = fetch_origin(workspace_path);

    // Checkout branch from origin
    let remote_ref = format!("origin/{}", branch);
    checkout_branch_from_base(workspace_path, branch, &remote_ref)?;

    // Hard reset and clean
    hard_reset_and_clean(workspace_path, &remote_ref)?;

    Ok(())
}

pub(crate) fn remove_workspace(
    workspace_root: &Path,
    spec: &WorkspaceSpec,
    force: bool,
) -> Result<()> {
    if !spec.path.exists() {
        return Ok(());
    }
    if !spec.path.starts_with(workspace_root) {
        bail!(
            "workspace path {} is outside root {}",
            spec.path.display(),
            workspace_root.display()
        );
    }
    if force {
        fs::remove_dir_all(&spec.path)
            .with_context(|| format!("remove workspace {}", spec.path.display()))?;
        return Ok(());
    }

    ensure_clean_workspace(&spec.path)?;
    fs::remove_dir_all(&spec.path)
        .with_context(|| format!("remove workspace {}", spec.path.display()))
}

fn clone_repo_from_local(repo_root: &Path, dest: &Path) -> Result<()> {
    let output = git_base_command(repo_root)
        .arg("clone")
        .arg("--no-hardlinks")
        .arg(".")
        .arg(dest)
        .output()
        .with_context(|| format!("run git clone into {}", dest.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git clone failed: {}", stderr.trim());
    }
    Ok(())
}

fn origin_urls(repo_root: &Path) -> Result<(String, String)> {
    let fetch = remote_url(repo_root, &["remote", "get-url", "origin"])?;
    let push = remote_url(repo_root, &["remote", "get-url", "--push", "origin"])?;

    match (fetch, push) {
        (Some(fetch_url), Some(push_url)) => Ok((fetch_url, push_url)),
        (Some(fetch_url), None) => Ok((fetch_url.clone(), fetch_url)),
        (None, Some(push_url)) => Ok((push_url.clone(), push_url)),
        (None, None) => {
            bail!("No 'origin' remote configured; parallel workspaces require a pushable origin.")
        }
    }
}

fn remote_url(repo_root: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = git_base_command(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn retarget_origin(workspace_path: &Path, fetch_url: &str, push_url: &str) -> Result<()> {
    let output = git_base_command(workspace_path)
        .arg("remote")
        .arg("set-url")
        .arg("origin")
        .arg(fetch_url.trim())
        .output()
        .with_context(|| format!("set origin fetch url in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git remote set-url origin failed: {}", stderr.trim());
    }

    let output = git_base_command(workspace_path)
        .arg("remote")
        .arg("set-url")
        .arg("--push")
        .arg("origin")
        .arg(push_url.trim())
        .output()
        .with_context(|| format!("set origin push url in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git remote set-url --push origin failed: {}", stderr.trim());
    }
    Ok(())
}

fn fetch_origin(workspace_path: &Path) -> Result<()> {
    let output = git_base_command(workspace_path)
        .arg("fetch")
        .arg("origin")
        .arg("--prune")
        .output()
        .with_context(|| format!("run git fetch in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git fetch failed: {}", stderr.trim());
    }
    Ok(())
}

fn resolve_base_ref(workspace_path: &Path, base_branch: &str) -> Result<String> {
    let remote_ref = format!("refs/remotes/origin/{}", base_branch);
    if git_ref_exists(workspace_path, &remote_ref)? {
        return Ok(format!("origin/{}", base_branch));
    }
    let local_ref = format!("refs/heads/{}", base_branch);
    if git_ref_exists(workspace_path, &local_ref)? {
        return Ok(base_branch.to_string());
    }
    bail!("base branch '{}' not found in workspace", base_branch);
}

fn git_ref_exists(repo_root: &Path, full_ref: &str) -> Result<bool> {
    let output = git_base_command(repo_root)
        .arg("show-ref")
        .arg("--verify")
        .arg("--quiet")
        .arg(full_ref)
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

fn checkout_branch_from_base(workspace_path: &Path, branch: &str, base_ref: &str) -> Result<()> {
    let output = git_base_command(workspace_path)
        .arg("checkout")
        .arg("-B")
        .arg(branch)
        .arg(base_ref)
        .output()
        .with_context(|| format!("run git checkout -B in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git checkout -B failed: {}", stderr.trim());
    }
    Ok(())
}

fn hard_reset_and_clean(workspace_path: &Path, base_ref: &str) -> Result<()> {
    let output = git_base_command(workspace_path)
        .arg("reset")
        .arg("--hard")
        .arg(base_ref)
        .output()
        .with_context(|| format!("run git reset in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git reset --hard failed: {}", stderr.trim());
    }

    let output = git_base_command(workspace_path)
        .arg("clean")
        .arg("-fd")
        .output()
        .with_context(|| format!("run git clean in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git clean failed: {}", stderr.trim());
    }
    Ok(())
}

fn ensure_clean_workspace(workspace_path: &Path) -> Result<()> {
    let output = git_base_command(workspace_path)
        .arg("status")
        .arg("--porcelain")
        .output()
        .with_context(|| format!("run git status in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {}", stderr.trim());
    }
    let status = String::from_utf8_lossy(&output.stdout);
    if !status.trim().is_empty() {
        bail!("workspace is dirty; use force to remove");
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
    fn workspace_root_uses_repo_root_for_relative_path() {
        let cfg = Config {
            parallel: ParallelConfig {
                workspace_root: Some(PathBuf::from(".ralph/workspaces/custom")),
                ..ParallelConfig::default()
            },
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = workspace_root(&repo_root, &cfg);
        assert_eq!(
            root,
            PathBuf::from("/tmp/ralph-test/.ralph/workspaces/custom")
        );
    }

    #[test]
    fn workspace_root_accepts_absolute_path() {
        let cfg = Config {
            parallel: ParallelConfig {
                workspace_root: Some(PathBuf::from("/tmp/ralph-workspaces")),
                ..ParallelConfig::default()
            },
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = workspace_root(&repo_root, &cfg);
        assert_eq!(root, PathBuf::from("/tmp/ralph-workspaces"));
    }

    #[test]
    fn workspace_root_defaults_outside_repo() {
        let cfg = Config {
            parallel: ParallelConfig::default(),
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = workspace_root(&repo_root, &cfg);
        assert_eq!(root, PathBuf::from("/tmp/.workspaces/ralph-test/parallel"));
    }

    #[test]
    fn create_and_remove_workspace_round_trips() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        )?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let root = temp.path().join(".ralph/workspaces/parallel");

        let spec = create_workspace_at(temp.path(), &root, "RQ-0001", &base_branch, "ralph/")?;
        assert!(spec.path.exists(), "workspace path should exist");

        remove_workspace(&root, &spec, true)?;
        assert!(!spec.path.exists());
        Ok(())
    }

    #[test]
    fn create_workspace_reuses_existing_and_cleans() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        )?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let root = temp.path().join(".ralph/workspaces/parallel");

        let first = create_workspace_at(temp.path(), &root, "RQ-0001", &base_branch, "ralph/")?;
        std::fs::write(first.path.join("dirty.txt"), "dirty")?;

        let second = create_workspace_at(temp.path(), &root, "RQ-0001", &base_branch, "ralph/")?;
        assert_eq!(first.path, second.path);
        assert!(!second.path.join("dirty.txt").exists());

        remove_workspace(&root, &second, true)?;
        Ok(())
    }

    #[test]
    fn create_workspace_with_existing_branch() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        )?;
        git_test::git_run(temp.path(), &["branch", "ralph/RQ-0002"])?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let root = temp.path().join(".ralph/workspaces/parallel");

        let spec = create_workspace_at(temp.path(), &root, "RQ-0002", &base_branch, "ralph/")?;
        assert!(spec.path.exists());

        remove_workspace(&root, &spec, true)?;
        Ok(())
    }

    #[test]
    fn create_workspace_requires_origin_remote() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let root = temp.path().join(".ralph/workspaces/parallel");

        let err = create_workspace_at(temp.path(), &root, "RQ-0003", &base_branch, "ralph/")
            .expect_err("missing origin should fail");
        assert!(err.to_string().contains("origin"));
        Ok(())
    }

    #[test]
    fn remove_workspace_requires_force_when_dirty() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        )?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let root = temp.path().join(".ralph/workspaces/parallel");

        let spec = create_workspace_at(temp.path(), &root, "RQ-0004", &base_branch, "ralph/")?;
        std::fs::write(spec.path.join("dirty.txt"), "dirty")?;
        let err = remove_workspace(&root, &spec, false).expect_err("dirty should fail");
        assert!(err.to_string().contains("dirty"));
        assert!(spec.path.exists());

        remove_workspace(&root, &spec, true)?;
        Ok(())
    }

    #[test]
    fn ensure_workspace_exists_creates_missing_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        )?;

        let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let workspace_path = temp.path().join("workspaces/RQ-0001");

        ensure_workspace_exists(temp.path(), &workspace_path, &branch)?;

        assert!(workspace_path.exists(), "workspace path should exist");
        assert!(
            workspace_path.join(".git").exists(),
            "workspace should be a git repo"
        );

        // Verify we're on the correct branch
        let current_branch =
            git_test::git_output(&workspace_path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        assert_eq!(current_branch, branch);

        Ok(())
    }

    #[test]
    fn ensure_workspace_exists_reuses_existing_and_cleans() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        )?;

        let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let workspace_path = temp.path().join("workspaces/RQ-0001");

        // First call creates the workspace
        ensure_workspace_exists(temp.path(), &workspace_path, &branch)?;

        // Add some dirty files
        std::fs::write(workspace_path.join("dirty.txt"), "dirty")?;
        std::fs::create_dir_all(workspace_path.join("untracked_dir"))?;
        std::fs::write(workspace_path.join("untracked_dir/file.txt"), "untracked")?;

        // Second call should clean up
        ensure_workspace_exists(temp.path(), &workspace_path, &branch)?;

        assert!(
            !workspace_path.join("dirty.txt").exists(),
            "dirty file should be cleaned"
        );
        assert!(
            !workspace_path.join("untracked_dir").exists(),
            "untracked dir should be cleaned"
        );

        Ok(())
    }

    #[test]
    fn ensure_workspace_exists_replaces_invalid_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        )?;

        let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let workspace_path = temp.path().join("workspaces/RQ-0001");

        // Create a non-git directory (invalid workspace)
        std::fs::create_dir_all(&workspace_path)?;
        std::fs::write(workspace_path.join("some_file.txt"), "content")?;

        ensure_workspace_exists(temp.path(), &workspace_path, &branch)?;

        assert!(
            workspace_path.join(".git").exists(),
            "workspace should be a valid git repo"
        );
        assert!(
            !workspace_path.join("some_file.txt").exists(),
            "old file should be gone"
        );

        Ok(())
    }

    #[test]
    fn ensure_workspace_exists_fails_without_origin() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        // Note: no origin remote added

        let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let workspace_path = temp.path().join("workspaces/RQ-0001");

        let err = ensure_workspace_exists(temp.path(), &workspace_path, &branch)
            .expect_err("should fail without origin");
        assert!(err.to_string().contains("origin"));

        Ok(())
    }
}
