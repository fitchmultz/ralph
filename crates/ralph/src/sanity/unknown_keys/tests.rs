//! Regression tests for unknown-config-key detection and repair helpers.
//!
//! Purpose:
//! - Regression tests for unknown-config-key detection and repair helpers.
//!
//! Responsibilities:
//! - Cover schema key extraction and unknown-key detection.
//! - Verify prompt/action parsing and config-file mutation edge cases.
//! - Confirm auto-fix behavior preserves supported config content.
//!
//! Not handled here:
//! - Broader sanity-check orchestration.
//! - Config migration flows outside unknown-key handling.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Unknown keys are reported in dot notation.
//! - Rename actions preserve operator-provided casing.
//! - Empty config files remain unchanged when mutation helpers run.

use super::*;
use tempfile::TempDir;

#[test]
fn get_known_config_keys_includes_top_level() {
    let keys = get_known_config_keys();
    assert!(keys.contains("version"));
    assert!(keys.contains("project_type"));
    assert!(keys.contains("agent"));
    assert!(keys.contains("queue"));
    assert!(keys.contains("parallel"));
    assert!(keys.contains("plugins"));
    assert!(keys.contains("profiles"));
}

#[test]
fn get_known_config_keys_includes_agent_keys() {
    let keys = get_known_config_keys();
    assert!(keys.contains("agent.runner"));
    assert!(keys.contains("agent.model"));
    assert!(keys.contains("agent.phases"));
    assert!(keys.contains("agent.codex_bin"));
    assert!(keys.contains("agent.followup_reasoning_effort"));
    assert!(keys.contains("agent.claude_permission_mode"));
    assert!(keys.contains("agent.runner_cli"));
    assert!(keys.contains("agent.notification"));
    assert!(keys.contains("agent.notification.enabled"));
    assert!(keys.contains("agent.notification.notify_on_complete"));
}

#[test]
fn get_known_config_keys_extracts_runner_cli_keys() {
    let keys = get_known_config_keys();
    assert!(keys.contains("agent.runner_cli"));
    assert!(keys.contains("agent.runner_cli.defaults"));
    assert!(keys.contains("agent.runner_cli.runners"));
}

#[test]
fn check_config_file_unknown_keys_detects_unknown() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    std::fs::write(
        &config_path,
        r#"{
                "version": 1,
                "unknown_key": "value",
                "agent": {
                    "runner": "claude",
                    "unknown_agent_key": 123
                }
            }"#,
    )
    .unwrap();

    let known_keys = get_known_config_keys();
    let unknown = check_config_file_unknown_keys(&config_path, &known_keys).unwrap();

    assert!(unknown.contains(&"unknown_key".to_string()));
    assert!(unknown.contains(&"agent.unknown_agent_key".to_string()));
    assert!(!unknown.contains(&"version".to_string()));
    assert!(!unknown.contains(&"agent.runner".to_string()));
}

#[test]
fn parse_unknown_key_action_preserves_rename_case() {
    assert!(matches!(
        parse_unknown_key_action(" Agent.Runner_CLI  "),
        UnknownKeyAction::Rename(rename) if rename == "Agent.Runner_CLI"
    ));
    assert!(matches!(
        parse_unknown_key_action("KEEP"),
        UnknownKeyAction::Keep
    ));
    assert!(matches!(
        parse_unknown_key_action("Remove"),
        UnknownKeyAction::Remove
    ));
}

#[test]
fn is_known_parent_key_detects_parents() {
    let keys = get_known_config_keys();
    assert!(is_known_parent_key("agent", &keys));
    assert!(is_known_parent_key("queue", &keys));
    assert!(!is_known_parent_key("unknown", &keys));
}

#[test]
fn remove_key_from_config_file_leaves_empty_file_unchanged() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(&config_path, "").unwrap();

    remove_key_in_file(&config_path, "agent.runner").unwrap();

    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), "");
}

#[test]
fn rename_key_in_config_file_leaves_empty_file_unchanged() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(&config_path, "").unwrap();

    rename_key_in_file(&config_path, "agent.runner", "agent.runner_cli").unwrap();

    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), "");
}

#[test]
fn rename_key_in_config_file_rejects_parent_path_changes() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(&config_path, r#"{ "parallel": { "worktree_root": "x" } }"#).unwrap();

    let err = rename_key_in_file(
        &config_path,
        "parallel.worktree_root",
        "agent.workspace_root",
    )
    .unwrap_err();

    assert!(err.to_string().contains("must keep the same parent path"));
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("worktree_root"));
    assert!(!content.contains("workspace_root"));
}

#[test]
fn check_unknown_keys_auto_fix_removes_unknown_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(
        &config_path,
        r#"{
                "version": 2,
                "unknown_key": "value"
            }"#,
    )
    .unwrap();

    let resolved = Resolved {
        config: crate::contracts::Config::default(),
        repo_root: dir.path().to_path_buf(),
        queue_path: dir.path().join("queue.json"),
        done_path: dir.path().join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(config_path.clone()),
    };

    let actions = check_unknown_keys(&resolved, true, true, || false).unwrap();
    assert!(
        actions
            .iter()
            .any(|action| action.contains("Removed unknown key 'unknown_key'"))
    );

    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(!content.contains("unknown_key"));
    assert!(content.contains("\"version\": 2"));
}
