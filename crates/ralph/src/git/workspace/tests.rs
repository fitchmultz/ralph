//! Tests for git workspace helpers.
//!
//! Purpose:
//! - Tests for git workspace helpers.
//!
//! Responsibilities:
//! - Verify workspace path resolution, clone/reset lifecycle, and removal safety.
//! - Keep regression coverage for origin retargeting and invalid workspace replacement.
//! - Isolate environment-variable-sensitive tests behind serialization.
//!
//! Not handled here:
//! - Parallel orchestration behavior outside git workspace management.
//! - PR or branch-push flows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests use temporary repositories and temp-root derived paths.
//! - HOME mutation tests hold a global lock for process safety.

use std::env;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use serial_test::serial;
use tempfile::TempDir;

use crate::contracts::{Config, ParallelConfig};
use crate::testsupport::git as git_test;

use super::{create_workspace_at, ensure_workspace_exists, remove_workspace, workspace_root};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn workspace_root_uses_repo_root_for_relative_path() {
    let cfg = Config {
        parallel: ParallelConfig {
            workspace_root: Some(PathBuf::from(".ralph/workspaces/custom")),
            ..ParallelConfig::default()
        },
        ..Config::default()
    };
    let repo_root = crate::testsupport::path::portable_abs_path("ralph-test");
    let root = workspace_root(&repo_root, &cfg);
    assert_eq!(root, repo_root.join(".ralph/workspaces/custom"));
}

#[test]
fn workspace_root_accepts_absolute_path() {
    let absolute_root = crate::testsupport::path::portable_abs_path("ralph-workspaces");
    let cfg = Config {
        parallel: ParallelConfig {
            workspace_root: Some(absolute_root.clone()),
            ..ParallelConfig::default()
        },
        ..Config::default()
    };
    let repo_root = crate::testsupport::path::portable_abs_path("ralph-test");
    let root = workspace_root(&repo_root, &cfg);
    assert_eq!(root, absolute_root);
}

#[test]
fn workspace_root_defaults_outside_repo() {
    let cfg = Config {
        parallel: ParallelConfig::default(),
        ..Config::default()
    };
    let repo_root = crate::testsupport::path::portable_abs_path("ralph-test");
    let root = workspace_root(&repo_root, &cfg);
    assert_eq!(
        root,
        repo_root
            .parent()
            .unwrap()
            .join(".workspaces")
            .join("ralph-test")
            .join("parallel")
    );
}

#[test]
fn create_and_remove_workspace_round_trips() -> Result<()> {
    let temp = seeded_repo()?;
    let base_branch = current_branch(temp.path())?;
    let root = temp.path().join(".ralph/workspaces/parallel");

    let spec = create_workspace_at(temp.path(), &root, "RQ-0001", &base_branch)?;
    assert!(spec.path.exists(), "workspace path should exist");
    assert_eq!(spec.branch, base_branch);

    remove_workspace(&root, &spec, true)?;
    assert!(!spec.path.exists());
    Ok(())
}

#[test]
fn create_workspace_reuses_existing_and_cleans() -> Result<()> {
    let temp = seeded_repo()?;
    let base_branch = current_branch(temp.path())?;
    let root = temp.path().join(".ralph/workspaces/parallel");

    let first = create_workspace_at(temp.path(), &root, "RQ-0001", &base_branch)?;
    std::fs::write(first.path.join("dirty.txt"), "dirty")?;

    let second = create_workspace_at(temp.path(), &root, "RQ-0001", &base_branch)?;
    assert_eq!(first.path, second.path);
    assert!(!second.path.join("dirty.txt").exists());
    assert_eq!(second.branch, base_branch);

    remove_workspace(&root, &second, true)?;
    Ok(())
}

#[test]
fn create_workspace_reuses_existing_with_conflicting_untracked_tracked_path() -> Result<()> {
    let temp = seeded_repo()?;
    std::fs::create_dir_all(temp.path().join(".ralph"))?;
    std::fs::write(temp.path().join(".ralph/config.jsonc"), "{tracked_config}")?;
    git_test::commit_all(temp.path(), "add tracked ralph config")?;

    let base_branch = current_branch(temp.path())?;
    let root = temp.path().join(".ralph/workspaces/parallel");
    let first = create_workspace_at(temp.path(), &root, "RQ-0009", &base_branch)?;

    git_test::git_run(&first.path, &["checkout", "-b", "stale-no-config"])?;
    git_test::git_run(&first.path, &["rm", "--cached", ".ralph/config.jsonc"])?;
    git_test::git_run(
        &first.path,
        &["commit", "-m", "drop tracked config in stale branch"],
    )?;

    // Leave an untracked file at a path that is tracked on base_branch.
    std::fs::write(first.path.join(".ralph/config.jsonc"), "{untracked_config}")?;

    let stale_status = git_test::git_output(
        &first.path,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    assert!(
        stale_status
            .lines()
            .any(|line| line.trim() == "?? .ralph/config.jsonc"),
        "expected stale branch to have untracked .ralph/config.jsonc, got: {stale_status}"
    );

    let second = create_workspace_at(temp.path(), &root, "RQ-0009", &base_branch)?;
    assert_eq!(first.path, second.path);
    let status_after = git_test::git_output(
        &second.path,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    assert!(
        status_after.trim().is_empty(),
        "expected clean workspace after reuse reset, got: {status_after}"
    );
    assert!(second.path.join(".ralph/config.jsonc").exists());

    remove_workspace(&root, &second, true)?;
    Ok(())
}

#[test]
fn create_workspace_with_existing_branch() -> Result<()> {
    let temp = seeded_repo()?;
    let base_branch = current_branch(temp.path())?;
    let root = temp.path().join(".ralph/workspaces/parallel");

    let spec = create_workspace_at(temp.path(), &root, "RQ-0002", &base_branch)?;
    assert!(spec.path.exists());
    assert_eq!(spec.branch, base_branch);

    remove_workspace(&root, &spec, true)?;
    Ok(())
}

#[test]
fn create_workspace_requires_origin_remote() -> Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    std::fs::write(temp.path().join("init.txt"), "init")?;
    git_test::commit_all(temp.path(), "init")?;

    let base_branch = current_branch(temp.path())?;
    let root = temp.path().join(".ralph/workspaces/parallel");

    let err = create_workspace_at(temp.path(), &root, "RQ-0003", &base_branch)
        .expect_err("missing origin should fail");
    assert!(err.to_string().contains("origin"));
    Ok(())
}

#[test]
fn remove_workspace_requires_force_when_dirty() -> Result<()> {
    let temp = seeded_repo()?;
    let base_branch = current_branch(temp.path())?;
    let root = temp.path().join(".ralph/workspaces/parallel");

    let spec = create_workspace_at(temp.path(), &root, "RQ-0004", &base_branch)?;
    std::fs::write(spec.path.join("dirty.txt"), "dirty")?;
    let err = remove_workspace(&root, &spec, false).expect_err("dirty should fail");
    assert!(err.to_string().contains("dirty"));
    assert!(spec.path.exists());

    remove_workspace(&root, &spec, true)?;
    Ok(())
}

#[test]
fn ensure_workspace_exists_creates_missing_workspace() -> Result<()> {
    let temp = seeded_repo()?;
    let branch = current_branch(temp.path())?;
    let workspace_path = temp.path().join("workspaces/RQ-0001");

    ensure_workspace_exists(temp.path(), &workspace_path, &branch)?;

    assert!(workspace_path.exists(), "workspace path should exist");
    assert!(
        workspace_path.join(".git").exists(),
        "workspace should be a git repo"
    );
    assert_eq!(current_branch(&workspace_path)?, branch);

    Ok(())
}

#[test]
fn ensure_workspace_exists_reuses_existing_and_cleans() -> Result<()> {
    let temp = seeded_repo()?;
    let branch = current_branch(temp.path())?;
    let workspace_path = temp.path().join("workspaces/RQ-0001");

    ensure_workspace_exists(temp.path(), &workspace_path, &branch)?;
    std::fs::write(workspace_path.join("dirty.txt"), "dirty")?;
    std::fs::create_dir_all(workspace_path.join("untracked_dir"))?;
    std::fs::write(workspace_path.join("untracked_dir/file.txt"), "untracked")?;

    ensure_workspace_exists(temp.path(), &workspace_path, &branch)?;

    assert!(!workspace_path.join("dirty.txt").exists());
    assert!(!workspace_path.join("untracked_dir").exists());
    Ok(())
}

#[test]
fn ensure_workspace_exists_replaces_invalid_workspace() -> Result<()> {
    let temp = seeded_repo()?;
    let branch = current_branch(temp.path())?;
    let workspace_path = temp.path().join("workspaces/RQ-0001");

    std::fs::create_dir_all(&workspace_path)?;
    std::fs::write(workspace_path.join("some_file.txt"), "content")?;

    ensure_workspace_exists(temp.path(), &workspace_path, &branch)?;

    assert!(workspace_path.join(".git").exists());
    assert!(!workspace_path.join("some_file.txt").exists());
    Ok(())
}

#[test]
fn ensure_workspace_exists_fails_without_origin() -> Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    std::fs::write(temp.path().join("init.txt"), "init")?;
    git_test::commit_all(temp.path(), "init")?;

    let branch = current_branch(temp.path())?;
    let workspace_path = temp.path().join("workspaces/RQ-0001");

    let err = ensure_workspace_exists(temp.path(), &workspace_path, &branch)
        .expect_err("should fail without origin");
    assert!(err.to_string().contains("origin"));
    Ok(())
}

#[test]
#[serial]
fn workspace_root_expands_tilde_to_home() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let cfg = Config {
        parallel: ParallelConfig {
            workspace_root: Some(PathBuf::from("~/ralph-workspaces")),
            ..ParallelConfig::default()
        },
        ..Config::default()
    };
    let repo_root = crate::testsupport::path::portable_abs_path("ralph-test");
    let root = workspace_root(&repo_root, &cfg);
    assert_eq!(root, PathBuf::from("/custom/home/ralph-workspaces"));

    restore_home(original_home);
}

#[test]
#[serial]
fn workspace_root_expands_tilde_alone_to_home() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let cfg = Config {
        parallel: ParallelConfig {
            workspace_root: Some(PathBuf::from("~")),
            ..ParallelConfig::default()
        },
        ..Config::default()
    };
    let repo_root = crate::testsupport::path::portable_abs_path("ralph-test");
    let root = workspace_root(&repo_root, &cfg);
    assert_eq!(root, PathBuf::from("/custom/home"));

    restore_home(original_home);
}

#[test]
#[serial]
fn workspace_root_relative_when_home_unset() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::remove_var("HOME") };

    let cfg = Config {
        parallel: ParallelConfig {
            workspace_root: Some(PathBuf::from("~/workspaces")),
            ..ParallelConfig::default()
        },
        ..Config::default()
    };
    let repo_root = crate::testsupport::path::portable_abs_path("ralph-test");
    let root = workspace_root(&repo_root, &cfg);
    assert_eq!(root, repo_root.join("~/workspaces"));

    restore_home(original_home);
}

fn seeded_repo() -> Result<TempDir> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    std::fs::write(temp.path().join("init.txt"), "init")?;
    git_test::commit_all(temp.path(), "init")?;
    git_test::git_run(
        temp.path(),
        &["remote", "add", "origin", "https://example.com/repo.git"],
    )?;
    Ok(temp)
}

fn current_branch(repo_root: &std::path::Path) -> Result<String> {
    git_test::git_output(repo_root, &["rev-parse", "--abbrev-ref", "HEAD"])
}

fn restore_home(original_home: Option<String>) {
    match original_home {
        Some(value) => unsafe { env::set_var("HOME", value) },
        None => unsafe { env::remove_var("HOME") },
    }
}
