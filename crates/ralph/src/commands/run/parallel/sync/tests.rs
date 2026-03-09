//! Tests for parallel workspace state synchronization.
//!
//! Responsibilities:
//! - Verify `.ralph` runtime sync behavior and bookkeeping seeding.
//! - Verify gitignored allowlist sync rules for worker workspaces.
//! - Preserve migration and custom-path edge cases for queue/done files.
//!
//! Does NOT handle:
//! - Worker lifecycle orchestration.
//! - Branch push/cleanup behavior outside sync helpers.
//!
//! Invariants:
//! - Tests use isolated temp git repos.
//! - Queue and done expectations are asserted from on-disk workspace state.

use super::*;
use crate::contracts::Config;
use crate::testsupport::git as git_test;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn build_test_resolved(
    repo_root: &Path,
    queue_path: Option<PathBuf>,
    done_path: Option<PathBuf>,
) -> config::Resolved {
    let queue_path = queue_path.unwrap_or_else(|| repo_root.join(".ralph/queue.json"));
    let done_path = done_path.unwrap_or_else(|| repo_root.join(".ralph/done.json"));
    config::Resolved {
        config: Config::default(),
        repo_root: repo_root.to_path_buf(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

#[test]
fn sync_ralph_state_copies_config_prompts_and_resolved_queue_done() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
    fs::create_dir_all(&workspace_root)?;
    fs::write(repo_root.join(".ralph/queue.json"), "{queue}")?;
    fs::write(repo_root.join(".ralph/done.json"), "{done}")?;
    fs::write(repo_root.join(".ralph/config.json"), "{config}")?;
    fs::write(repo_root.join(".ralph/prompts/override.md"), "prompt")?;

    let resolved = build_test_resolved(&repo_root, None, None);
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/queue.json"))?,
        "{queue}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/done.json"))?,
        "{done}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/config.json"))?,
        "{config}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/prompts/override.md"))?,
        "prompt"
    );
    Ok(())
}

#[test]
fn sync_ralph_state_copies_runtime_files_from_ignored_ralph_dir() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::write(repo_root.join(".gitignore"), ".ralph/\n")?;
    fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
    fs::create_dir_all(repo_root.join(".ralph/templates"))?;
    fs::create_dir_all(repo_root.join(".ralph/cache/parallel"))?;
    fs::create_dir_all(repo_root.join(".ralph/logs"))?;
    fs::create_dir_all(repo_root.join(".ralph/workspaces"))?;
    fs::create_dir_all(repo_root.join(".ralph/lock"))?;
    fs::write(repo_root.join(".ralph/queue.json"), "{queue}")?;
    fs::write(repo_root.join(".ralph/done.json"), "{done}")?;
    fs::write(
        repo_root.join(".ralph/config.jsonc"),
        "{/*comment*/\"version\":1}",
    )?;
    fs::write(
        repo_root.join(".ralph/prompts/worker.md"),
        "# Worker prompt",
    )?;
    fs::write(
        repo_root.join(".ralph/templates/task.json"),
        "{\"template\":true}",
    )?;
    fs::write(
        repo_root.join(".ralph/cache/parallel/state.json"),
        "{\"cached\":true}",
    )?;
    fs::write(repo_root.join(".ralph/logs/debug.log"), "debug")?;
    fs::write(
        repo_root.join(".ralph/workspaces/shared.txt"),
        "workspace state",
    )?;
    fs::write(repo_root.join(".ralph/lock/queue.lock"), "lock")?;

    let resolved = build_test_resolved(&repo_root, None, None);
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/config.jsonc"))?,
        "{/*comment*/\"version\":1}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/prompts/worker.md"))?,
        "# Worker prompt"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/templates/task.json"))?,
        "{\"template\":true}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/queue.json"))?,
        "{queue}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/done.json"))?,
        "{done}"
    );
    assert!(!workspace_root.join(".ralph/cache").exists());
    assert!(!workspace_root.join(".ralph/logs").exists());
    assert!(!workspace_root.join(".ralph/workspaces").exists());
    assert!(!workspace_root.join(".ralph/lock").exists());

    Ok(())
}

#[test]
fn sync_ralph_state_copies_jsonc_config_with_agent_overrides() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::create_dir_all(repo_root.join(".ralph"))?;
    fs::write(repo_root.join(".ralph/queue.jsonc"), "{queue}")?;
    fs::write(repo_root.join(".ralph/done.jsonc"), "{done}")?;
    fs::write(
        repo_root.join(".ralph/config.jsonc"),
        r#"{
  "version": 1,
  "agent": {
    "runner": "opencode",
    "model": "gpt-5.2",
    "phases": 3,
    "phase_overrides": {
      "phase1": { "runner": "codex", "model": "gpt-5.2-codex", "reasoning_effort": "high" },
      "phase2": { "runner": "claude", "model": "opus" },
      "phase3": { "runner": "gemini", "model": "gemini-3-pro-preview" }
    }
  }
}"#,
    )?;

    let resolved = build_test_resolved(
        &repo_root,
        Some(repo_root.join(".ralph/queue.jsonc")),
        Some(repo_root.join(".ralph/done.jsonc")),
    );
    sync_ralph_state(&resolved, &workspace_root)?;

    let config_json = fs::read_to_string(workspace_root.join(".ralph/config.jsonc"))?;
    let config: serde_json::Value = serde_json::from_str(&config_json)?;
    assert_eq!(config["agent"]["runner"], "opencode");
    assert_eq!(config["agent"]["model"], "gpt-5.2");
    assert_eq!(config["agent"]["phases"], 3);
    assert_eq!(
        config["agent"]["phase_overrides"]["phase1"]["runner"],
        "codex"
    );
    assert_eq!(
        config["agent"]["phase_overrides"]["phase2"]["model"],
        "opus"
    );
    assert_eq!(
        config["agent"]["phase_overrides"]["phase3"]["runner"],
        "gemini"
    );

    Ok(())
}

#[test]
fn sync_ralph_state_copies_allowlisted_env_files_but_skips_ignored_dirs() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::write(
        repo_root.join(".gitignore"),
        ".env\n.env.local\ntarget/\n.ralph/cache/parallel/\n",
    )?;
    fs::write(repo_root.join(".env"), "secret")?;
    fs::write(repo_root.join(".env.local"), "local_secret")?;
    fs::create_dir_all(repo_root.join("target"))?;
    fs::write(
        repo_root.join("target/very_large_file.txt"),
        "heavy build output",
    )?;
    fs::create_dir_all(repo_root.join(".ralph/cache/parallel"))?;
    fs::write(
        repo_root.join(".ralph/cache/parallel/state.json"),
        "{\"cached\": true}",
    )?;

    let resolved = build_test_resolved(&repo_root, None, None);
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(fs::read_to_string(workspace_root.join(".env"))?, "secret");
    assert_eq!(
        fs::read_to_string(workspace_root.join(".env.local"))?,
        "local_secret"
    );
    assert!(!workspace_root.join("target").exists());
    assert!(!workspace_root.join(".ralph/cache/parallel").exists());
    Ok(())
}

#[test]
fn sync_ralph_state_skips_parent_of_workspace() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = repo_root.join(".ralph/workspaces/RQ-0001");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::write(repo_root.join(".gitignore"), ".ralph/workspaces/\n")?;
    fs::create_dir_all(repo_root.join(".ralph/workspaces"))?;
    fs::write(
        repo_root.join(".ralph/workspaces/shared.txt"),
        "shared ignored",
    )?;
    fs::create_dir_all(&workspace_root)?;

    let resolved = build_test_resolved(&repo_root, None, None);
    sync_ralph_state(&resolved, &workspace_root)?;

    assert!(!workspace_root.join(".ralph/workspaces/shared.txt").exists());
    Ok(())
}

#[test]
fn sync_ralph_state_custom_queue_done_paths_are_synced() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;

    let queue_path = repo_root.join("queue/active.json");
    let done_path = repo_root.join("archive/done.json");
    fs::create_dir_all(queue_path.parent().unwrap())?;
    fs::create_dir_all(done_path.parent().unwrap())?;
    fs::write(&queue_path, "{custom_queue}")?;
    fs::write(&done_path, "{custom_done}")?;

    fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
    fs::write(repo_root.join(".ralph/config.json"), "{config}")?;
    fs::write(repo_root.join(".ralph/prompts/override.md"), "prompt")?;
    fs::create_dir_all(&workspace_root)?;

    let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("queue/active.json"))?,
        "{custom_queue}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join("archive/done.json"))?,
        "{custom_done}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/config.json"))?,
        "{config}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/prompts/override.md"))?,
        "prompt"
    );
    Ok(())
}

#[test]
fn sync_ralph_state_custom_queue_done_paths_inside_ralph_are_synced() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;

    let queue_path = repo_root.join(".ralph/data/queue.jsonc");
    let done_path = repo_root.join(".ralph/data/done.json");
    fs::create_dir_all(queue_path.parent().unwrap())?;
    fs::write(&queue_path, "{custom_queue}")?;
    fs::write(&done_path, "{custom_done}")?;

    fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
    fs::write(repo_root.join(".ralph/config.jsonc"), "{config}")?;
    fs::write(repo_root.join(".ralph/prompts/override.md"), "prompt")?;
    fs::create_dir_all(&workspace_root)?;

    let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/data/queue.jsonc"))?,
        "{custom_queue}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/data/done.json"))?,
        "{custom_done}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/config.jsonc"))?,
        "{config}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/prompts/override.md"))?,
        "prompt"
    );
    Ok(())
}

#[test]
fn sync_ralph_state_missing_done_file_allowed() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;

    let queue_path = repo_root.join("queue/active.json");
    fs::create_dir_all(queue_path.parent().unwrap())?;
    fs::write(&queue_path, "{queue}")?;

    fs::create_dir_all(repo_root.join(".ralph"))?;
    fs::write(repo_root.join(".ralph/config.json"), "{config}")?;
    fs::create_dir_all(&workspace_root)?;

    let done_path = repo_root.join("archive/done.json");
    let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("queue/active.json"))?,
        "{queue}"
    );
    assert!(!workspace_root.join("archive/done.json").exists());
    Ok(())
}

#[test]
fn sync_ralph_state_seeds_jsonc_bookkeeping_for_migrated_uncommitted_config() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;

    fs::create_dir_all(repo_root.join(".ralph"))?;
    fs::write(repo_root.join(".ralph/queue.json"), "{legacy_queue}")?;
    fs::write(repo_root.join(".ralph/done.json"), "{legacy_done}")?;
    fs::write(repo_root.join(".ralph/queue.jsonc"), "{migrated_queue}")?;
    fs::write(repo_root.join(".ralph/done.jsonc"), "{migrated_done}")?;
    fs::write(repo_root.join(".ralph/config.jsonc"), "{config}")?;

    fs::create_dir_all(workspace_root.join(".ralph"))?;
    fs::write(
        workspace_root.join(".ralph/queue.json"),
        "{legacy_workspace_queue}",
    )?;
    fs::write(
        workspace_root.join(".ralph/done.json"),
        "{legacy_workspace_done}",
    )?;

    let resolved = build_test_resolved(
        &repo_root,
        Some(repo_root.join(".ralph/queue.jsonc")),
        Some(repo_root.join(".ralph/done.jsonc")),
    );
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/queue.jsonc"))?,
        "{migrated_queue}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/done.jsonc"))?,
        "{migrated_done}"
    );
    assert!(workspace_root.join(".ralph/queue.json").exists());
    assert!(workspace_root.join(".ralph/done.json").exists());

    Ok(())
}

#[test]
fn should_sync_gitignored_entry_skips_empty() {
    assert!(!gitignored::should_sync_gitignored_entry(""));
}

#[test]
fn should_sync_gitignored_entry_skips_directories() {
    assert!(!gitignored::should_sync_gitignored_entry("target/"));
    assert!(!gitignored::should_sync_gitignored_entry("ignored_dir/"));
    assert!(!gitignored::should_sync_gitignored_entry("node_modules/"));
}

#[test]
fn should_sync_gitignored_entry_allows_env_files() {
    assert!(gitignored::should_sync_gitignored_entry(".env"));
    assert!(gitignored::should_sync_gitignored_entry(".env.local"));
    assert!(gitignored::should_sync_gitignored_entry(".env.production"));
    assert!(gitignored::should_sync_gitignored_entry(".env.development"));
}

#[test]
fn should_sync_gitignored_entry_allows_nested_env_files() {
    assert!(gitignored::should_sync_gitignored_entry("nested/.env"));
    assert!(gitignored::should_sync_gitignored_entry(
        "nested/.env.production"
    ));
    assert!(gitignored::should_sync_gitignored_entry(
        "config/.env.local"
    ));
}

#[test]
fn should_sync_gitignored_entry_skips_non_env_files() {
    assert!(!gitignored::should_sync_gitignored_entry("not_env.txt"));
    assert!(!gitignored::should_sync_gitignored_entry("README.md"));
    assert!(!gitignored::should_sync_gitignored_entry("secret.key"));
}

#[test]
fn should_sync_gitignored_entry_skips_never_copy_prefixes() {
    assert!(!gitignored::should_sync_gitignored_entry(
        "target/debug/app"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        "target/release/lib.rlib"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        "node_modules/lodash/index.js"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".venv/bin/python"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".ralph/cache/parallel/state.json"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".ralph/cache/plans/RQ-0001.md"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".ralph/workspaces/RQ-0001/.env"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".ralph/logs/run.log"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".ralph/lock/sync.lock"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        "__pycache__/module.cpython-311.pyc"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".ruff_cache/0.1.0/content"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".pytest_cache/v/cache/nodeids"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".ty_cache/some_file"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(".git/config"));
    assert!(!gitignored::should_sync_gitignored_entry(
        ".git/objects/abc"
    ));
}

#[test]
fn should_sync_gitignored_entry_normalizes_leading_dot_slash() {
    assert!(gitignored::should_sync_gitignored_entry("./.env"));
    assert!(gitignored::should_sync_gitignored_entry("./.env.local"));
    assert!(!gitignored::should_sync_gitignored_entry(
        "./target/debug/app"
    ));
    assert!(!gitignored::should_sync_gitignored_entry(
        "./node_modules/lodash"
    ));
}
