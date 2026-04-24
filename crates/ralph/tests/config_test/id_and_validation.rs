//! ID resolution and config validation tests.
//!
//! Purpose:
//! - ID resolution and config validation tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;

#[test]
fn test_resolve_id_prefix_default() {
    let cfg = Config::default();
    let prefix = config::resolve_id_prefix(&cfg).unwrap();
    assert_eq!(prefix, "RQ");
}

#[test]
fn test_resolve_id_prefix_custom() {
    let mut cfg = Config::default();
    cfg.queue.id_prefix = Some("TASK".to_string());
    let prefix = config::resolve_id_prefix(&cfg).unwrap();
    assert_eq!(prefix, "TASK");
}

#[test]
fn test_resolve_id_prefix_uppercases() {
    let mut cfg = Config::default();
    cfg.queue.id_prefix = Some("task".to_string());
    let prefix = config::resolve_id_prefix(&cfg).unwrap();
    assert_eq!(prefix, "TASK");
}

#[test]
fn test_resolve_id_prefix_trims_whitespace() {
    let mut cfg = Config::default();
    cfg.queue.id_prefix = Some("  TASK  ".to_string());
    let prefix = config::resolve_id_prefix(&cfg).unwrap();
    assert_eq!(prefix, "TASK");
}

#[test]
fn test_resolve_id_prefix_empty_fails() {
    let mut cfg = Config::default();
    cfg.queue.id_prefix = Some("".to_string());
    let result = config::resolve_id_prefix(&cfg);
    assert!(result.is_err());
}

#[test]
fn test_resolve_id_prefix_whitespace_only_fails() {
    let mut cfg = Config::default();
    cfg.queue.id_prefix = Some("   ".to_string());
    let result = config::resolve_id_prefix(&cfg);
    assert!(result.is_err());
}

#[test]
fn test_resolve_id_width_default() {
    let cfg = Config::default();
    let width = config::resolve_id_width(&cfg).unwrap();
    assert_eq!(width, 4);
}

#[test]
fn test_resolve_id_width_custom() {
    let mut cfg = Config::default();
    cfg.queue.id_width = Some(6);
    let width = config::resolve_id_width(&cfg).unwrap();
    assert_eq!(width, 6);
}

#[test]
fn test_resolve_id_width_zero_fails() {
    let mut cfg = Config::default();
    cfg.queue.id_width = Some(0);
    let result = config::resolve_id_width(&cfg);
    assert!(result.is_err());
}

#[test]
fn test_validate_config_version_unsupported() {
    let cfg = Config {
        version: 1,
        ..Default::default()
    };
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Unsupported config version"));
}

#[test]
fn test_validate_config_empty_id_prefix_fails() {
    let mut cfg = Config::default();
    cfg.queue.id_prefix = Some("".to_string());
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Empty queue.id_prefix"));
}

#[test]
fn test_validate_config_whitespace_id_prefix_fails() {
    let mut cfg = Config::default();
    cfg.queue.id_prefix = Some("   ".to_string());
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
}

#[test]
fn test_validate_config_zero_id_width_fails() {
    let mut cfg = Config::default();
    cfg.queue.id_width = Some(0);
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Invalid queue.id_width"));
}

#[test]
fn test_validate_config_empty_queue_file_fails() {
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from(""));
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Empty queue.file"));
}

#[test]
fn test_validate_config_empty_done_file_fails() {
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(PathBuf::from(""));
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Empty queue.done_file"));
}

#[test]
fn test_validate_config_empty_codex_bin_fails() {
    let mut cfg = Config::default();
    cfg.agent.codex_bin = Some("".to_string());
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Empty agent.codex_bin"));
}

#[test]
fn test_validate_config_empty_opencode_bin_fails() {
    let mut cfg = Config::default();
    cfg.agent.opencode_bin = Some("".to_string());
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Empty agent.opencode_bin"));
}

#[test]
fn test_validate_config_empty_gemini_bin_fails() {
    let mut cfg = Config::default();
    cfg.agent.gemini_bin = Some("".to_string());
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Empty agent.gemini_bin"));
}

#[test]
fn test_validate_config_empty_claude_bin_fails() {
    let mut cfg = Config::default();
    cfg.agent.claude_bin = Some("".to_string());
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Empty agent.claude_bin"));
}

#[test]
fn test_validate_config_valid_defaults() {
    let cfg = Config::default();
    let result = config::validate_config(&cfg);
    assert!(result.is_ok());
}

// Branch prefix tests removed in direct-push rewrite - branch_prefix config key no longer exists
