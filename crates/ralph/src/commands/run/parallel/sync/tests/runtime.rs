//! Runtime-tree sync tests for parallel workspace state synchronization.
//!
//! Purpose:
//! - Runtime-tree sync tests for parallel workspace state synchronization.
//!
//! Responsibilities:
//! - Verify `.ralph` runtime files are copied into worker workspaces.
//! - Verify runtime exclusions for ephemeral `.ralph` directories.
//! - Verify config/prompt sync scenarios remain intact.
//!
//! Non-scope:
//! - Custom queue/done path mapping edge cases.
//! - Gitignored allowlist unit coverage.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Test names and assertions match the prior flat suite exactly.
//! - Runtime expectations are asserted from on-disk workspace state.

use super::*;

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
    "model": "gpt-5.3",
    "phases": 3,
    "phase_overrides": {
      "phase1": { "runner": "codex", "model": "gpt-5.3-codex", "reasoning_effort": "high" },
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
    assert_eq!(config["agent"]["model"], "gpt-5.3");
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
