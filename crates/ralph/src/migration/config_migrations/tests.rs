//! Purpose: Regression coverage for the config migration facade and companions.
//!
//! Responsibilities:
//! - Preserve the original config migration unit coverage after the module split.
//! - Exercise both facade re-exports and internal helper behavior.
//!
//! Scope:
//! - Test-only module; production behavior lives in sibling companion modules.
//!
//! Usage:
//! - Compiled only under `#[cfg(test)]` from `config_migrations/mod.rs`.
//!
//! Invariants/Assumptions:
//! - Test names, assertions, fixtures, and behavior remain unchanged from the
//!   former inline config migration test block.
//! - Only import paths should change as needed for the new module layout.

use super::ci_gate::rewrite_ci_gate_in_file;
use super::detect::config_file_has_key;
use super::keys::{remove_key_in_file, rename_key_in_file, rename_key_in_text};
use super::legacy::{config_file_needs_legacy_contract_upgrade, upgrade_legacy_contract_in_file};
use std::fs;
use tempfile::TempDir;

#[test]
fn config_file_has_key_detects_existing_key() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(
        &config_path,
        r#"{
                "version": 1,
                "agent": {
                    "runner": "claude"
                }
            }"#,
    )
    .unwrap();

    assert!(config_file_has_key(&config_path, "version").unwrap());
    assert!(config_file_has_key(&config_path, "agent.runner").unwrap());
    assert!(!config_file_has_key(&config_path, "nonexistent").unwrap());
    assert!(!config_file_has_key(&config_path, "agent.nonexistent").unwrap());
}

#[test]
fn config_file_has_key_handles_jsonc() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(
        &config_path,
        r#"{
                // This is a comment
                "version": 1,
                "agent": {
                    "runner": "claude" // inline comment
                }
            }"#,
    )
    .unwrap();

    assert!(config_file_has_key(&config_path, "version").unwrap());
    assert!(config_file_has_key(&config_path, "agent.runner").unwrap());
}

#[test]
fn rename_key_in_file_works_with_simple_key() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(
        &config_path,
        r#"{
                "version": 1,
                "old_key": "value"
            }"#,
    )
    .unwrap();

    rename_key_in_file(&config_path, "old_key", "new_key").unwrap();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("\"new_key\""));
    assert!(!content.contains("\"old_key\""));
}

#[test]
fn rename_key_preserves_comments() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(
        &config_path,
        r#"{
                // Version comment
                "version": 1,
                /* Multi-line
                   comment */
                "old_key": "value"
            }"#,
    )
    .unwrap();

    rename_key_in_file(&config_path, "old_key", "new_key").unwrap();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("// Version comment"));
    assert!(content.contains("/* Multi-line"));
    assert!(content.contains("\"new_key\""));
    assert!(!content.contains("\"old_key\""));
}

#[test]
fn rename_key_in_text_finds_quoted_key() {
    let raw = r#"{"version": 1, "old_key": "value"}"#;
    let result = rename_key_in_text(raw, "old_key", "new_key").unwrap();
    assert!(result.contains("\"new_key\""));
    assert!(!result.contains("\"old_key\""));
}

#[test]
fn rename_key_in_text_preserves_non_key_occurrences() {
    let raw = r#"{"key": "old_key", "old_key": "value"}"#;
    let result = rename_key_in_text(raw, "old_key", "new_key").unwrap();
    assert!(result.contains("\"new_key\": \"value\""));
    assert!(result.contains("\"key\": \"old_key\""));
}

#[test]
fn rename_key_in_text_handles_whitespace() {
    let raw = r#"{
            "old_key"  : "value"
        }"#;
    let result = rename_key_in_text(raw, "old_key", "new_key").unwrap();
    assert!(result.contains("\"new_key\""));
    assert!(!result.contains("\"old_key\""));
}

#[test]
fn rename_key_in_text_escapes_single_quoted_replacement() {
    let raw = "{ 'old_key': 1 }";
    let result = rename_key_in_text(raw, "old_key", "new'key").unwrap();
    assert_eq!(result, "{ 'new\\'key': 1 }");
}

#[test]
fn rename_key_in_file_uses_leaf_of_dot_path_keys() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(&config_path, r#"{"parallel":{"worktree_root":"x"}}"#).unwrap();

    rename_key_in_file(
        &config_path,
        "parallel.worktree_root",
        "parallel.workspace_root",
    )
    .unwrap();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("\"workspace_root\""));
    assert!(!content.contains("\"worktree_root\""));
    assert!(!content.contains("\"parallel.workspace_root\""));
}

#[test]
fn rename_key_in_file_rejects_parent_path_changes() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(&config_path, r#"{"parallel":{"worktree_root":"x"}}"#).unwrap();

    let err = rename_key_in_file(
        &config_path,
        "parallel.worktree_root",
        "agent.workspace_root",
    )
    .unwrap_err();

    assert!(err.to_string().contains("must keep the same parent path"));
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("\"worktree_root\""));
    assert!(!content.contains("\"workspace_root\""));
}

#[test]
fn rename_key_in_file_handles_bare_top_level_key_and_quotes_new_name() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(&config_path, "{ old_key: 1 }").unwrap();

    rename_key_in_file(&config_path, "old_key", "new-key").unwrap();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("\"new-key\": 1"));
    assert!(!content.contains("old_key"));
}

#[test]
fn rename_key_in_file_preserves_single_quoted_top_level_key() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(&config_path, "{ 'old_key': 1 }").unwrap();

    rename_key_in_file(&config_path, "old_key", "new'key").unwrap();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("'new\\'key': 1"));
    assert!(!content.contains("old_key"));
}

#[test]
fn rename_key_scoped_to_parent_object() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(
        &config_path,
        r#"{
                "parallel": {
                    "worktree_root": "/tmp/parallel"
                },
                "other": {
                    "worktree_root": "/tmp/other"
                }
            }"#,
    )
    .unwrap();

    rename_key_in_file(
        &config_path,
        "parallel.worktree_root",
        "parallel.workspace_root",
    )
    .unwrap();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(
        content
            .contains("\"parallel\": {\n                    \"workspace_root\": \"/tmp/parallel\"")
    );
    assert!(
        content.contains("\"other\": {\n                    \"worktree_root\": \"/tmp/other\"")
    );
}

#[test]
fn rename_key_scoped_with_comments() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(
        &config_path,
        r#"{
                // Parallel execution settings { keep }
                "parallel": {
                    /* old setting name { keep } */
                    "worktree_root": "/tmp/worktrees"
                }
            }"#,
    )
    .unwrap();

    rename_key_in_file(
        &config_path,
        "parallel.worktree_root",
        "parallel.workspace_root",
    )
    .unwrap();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("\"workspace_root\": \"/tmp/worktrees\""));
    assert!(!content.contains("\"worktree_root\""));
    assert!(content.contains("// Parallel execution settings { keep }"));
    assert!(content.contains("/* old setting name { keep } */"));
}

#[test]
fn rename_key_scoped_to_nested_parent_path() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(
        &config_path,
        r#"{
                "agent": {
                    "notification": {
                        "sound_file": "nested",
                        "notification": {
                            "sound_file": "deeper"
                        }
                    },
                    "other": {
                        "sound_file": "sibling"
                    }
                },
                "notification": {
                    "sound_file": "top-level"
                }
            }"#,
    )
    .unwrap();

    rename_key_in_file(
        &config_path,
        "agent.notification.sound_file",
        "agent.notification.sound_path",
    )
    .unwrap();

    let value = jsonc_parser::parse_to_serde_value::<serde_json::Value>(
        &fs::read_to_string(&config_path).unwrap(),
        &Default::default(),
    )
    .unwrap();

    assert_eq!(
        value
            .pointer("/agent/notification/sound_path")
            .and_then(|v| v.as_str()),
        Some("nested")
    );
    assert_eq!(
        value
            .pointer("/agent/notification/notification/sound_file")
            .and_then(|v| v.as_str()),
        Some("deeper")
    );
    assert_eq!(
        value
            .pointer("/agent/other/sound_file")
            .and_then(|v| v.as_str()),
        Some("sibling")
    );
    assert_eq!(
        value
            .pointer("/notification/sound_file")
            .and_then(|v| v.as_str()),
        Some("top-level")
    );
}

#[test]
fn remove_key_in_file_removes_nested_key() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(
        &config_path,
        r#"{
                "version": 1,
                "agent": {
                    "runner": "claude",
                    "update_task_before_run": true
                }
            }"#,
    )
    .unwrap();

    remove_key_in_file(&config_path, "agent.update_task_before_run").unwrap();

    let value = jsonc_parser::parse_to_serde_value::<serde_json::Value>(
        &fs::read_to_string(&config_path).unwrap(),
        &Default::default(),
    )
    .unwrap();
    let agent = value.get("agent").unwrap();
    assert!(agent.get("update_task_before_run").is_none());
    assert_eq!(agent.get("runner").and_then(|v| v.as_str()), Some("claude"));
}

#[test]
fn remove_key_in_file_leaves_empty_file_unchanged() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");

    fs::write(&config_path, "").unwrap();

    remove_key_in_file(&config_path, "agent.update_task_before_run").unwrap();

    assert_eq!(fs::read_to_string(&config_path).unwrap(), "");
}

#[test]
fn legacy_contract_upgrade_detects_version_one_without_legacy_flag() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    fs::write(&config_path, r#"{"version":1,"agent":{"runner":"codex"}}"#).unwrap();

    assert!(config_file_needs_legacy_contract_upgrade(&config_path).unwrap());
}

#[test]
fn legacy_contract_upgrade_rewrites_publish_flag_to_git_publish_mode() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    fs::write(
        &config_path,
        r#"{"version":1,"agent":{"git_commit_push_enabled":true}}"#,
    )
    .unwrap();

    upgrade_legacy_contract_in_file(&config_path).unwrap();

    let value = jsonc_parser::parse_to_serde_value::<serde_json::Value>(
        &fs::read_to_string(&config_path).unwrap(),
        &Default::default(),
    )
    .unwrap();
    let agent = value.get("agent").unwrap();
    assert_eq!(value.get("version").and_then(|v| v.as_u64()), Some(2));
    assert!(agent.get("git_commit_push_enabled").is_none());
    assert_eq!(
        agent.get("git_publish_mode").and_then(|v| v.as_str()),
        Some("commit_and_push")
    );
}

#[test]
fn legacy_contract_upgrade_preserves_existing_git_publish_mode() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    fs::write(
        &config_path,
        r#"{"version":1,"agent":{"git_commit_push_enabled":false,"git_publish_mode":"commit"}}"#,
    )
    .unwrap();

    upgrade_legacy_contract_in_file(&config_path).unwrap();

    let value = jsonc_parser::parse_to_serde_value::<serde_json::Value>(
        &fs::read_to_string(&config_path).unwrap(),
        &Default::default(),
    )
    .unwrap();
    let agent = value.get("agent").unwrap();
    assert_eq!(value.get("version").and_then(|v| v.as_u64()), Some(2));
    assert!(agent.get("git_commit_push_enabled").is_none());
    assert_eq!(
        agent.get("git_publish_mode").and_then(|v| v.as_str()),
        Some("commit")
    );
}

#[test]
fn ci_gate_rewrite_migrates_simple_legacy_command_in_file() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.jsonc");
    fs::write(
        &config_path,
        r#"{
  "version": 2,
  "agent": {
    "runner": "claude",
    "ci_gate_command": "make ci",
    "ci_gate_enabled": true
  }
}"#,
    )
    .unwrap();

    rewrite_ci_gate_in_file(&config_path).unwrap();

    let value = jsonc_parser::parse_to_serde_value::<serde_json::Value>(
        &fs::read_to_string(&config_path).unwrap(),
        &Default::default(),
    )
    .unwrap();
    let agent = value.get("agent").unwrap();
    assert!(agent.get("ci_gate_command").is_none());
    assert!(agent.get("ci_gate_enabled").is_none());
    let gate = agent.get("ci_gate").unwrap();
    assert_eq!(gate.get("enabled").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        gate.get("argv").cloned(),
        Some(serde_json::json!(["make", "ci"]))
    );
}

#[test]
fn ci_gate_rewrite_leaves_file_unchanged_on_compound_legacy_command() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.jsonc");
    let original = r#"{
  "version": 2,
  "agent": {
    "ci_gate_command": "cargo test && cargo clippy",
    "ci_gate_enabled": true
  }
}"#;
    fs::write(&config_path, original).unwrap();

    let err = rewrite_ci_gate_in_file(&config_path).unwrap_err();
    assert!(
        err.to_string()
            .contains("cannot migrate legacy agent.ci_gate_command"),
        "{err}"
    );

    assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
}
