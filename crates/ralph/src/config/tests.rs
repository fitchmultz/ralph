//! Tests for configuration module.
//!
//! Responsibilities:
//! - Test layer application and merging.
//! - Test config validation.
//! - Test path resolution.
//! - Test instruction_files validation.
//! - Test queue validation consistency.
//!
//! Not handled here:
//! - Integration tests (see `tests/` directory).

use super::super::contracts::{Config, GitPublishMode, GitRevertMode};
use super::super::prompts_internal::validate_instruction_file_paths;
use super::RepoTrust;
use super::layer::{ConfigLayer, apply_layer, load_layer, save_layer};
use super::resolution::{
    resolve_done_path, resolve_id_prefix, resolve_id_width, resolve_queue_path,
};
use super::validation::{
    ERR_EMPTY_QUEUE_DONE_FILE, ERR_EMPTY_QUEUE_FILE, ERR_EMPTY_QUEUE_ID_PREFIX,
    ERR_INVALID_QUEUE_ID_WIDTH, ERR_PROJECT_EXECUTION_TRUST, validate_config,
    validate_project_execution_trust,
};
use anyhow::Result;
use std::path::PathBuf;

#[test]
fn apply_layer_overrides_git_revert_mode() -> Result<()> {
    let base = Config::default();
    let mut layer = ConfigLayer::default();
    layer.agent.git_revert_mode = Some(GitRevertMode::Disabled);

    let merged = apply_layer(base, layer)?;
    assert_eq!(
        merged.agent.git_revert_mode.unwrap_or(GitRevertMode::Ask),
        GitRevertMode::Disabled
    );
    Ok(())
}

#[test]
fn apply_layer_overrides_git_publish_mode() -> Result<()> {
    let base = Config::default();
    let mut layer = ConfigLayer::default();
    layer.agent.git_publish_mode = Some(GitPublishMode::Commit);

    let merged = apply_layer(base, layer)?;
    assert_eq!(merged.agent.git_publish_mode, Some(GitPublishMode::Commit));
    Ok(())
}

#[test]
fn save_layer_writes_version_and_round_trips() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let path = temp.path().join("config.jsonc");
    let layer = ConfigLayer::default();

    save_layer(&path, &layer)?;
    let loaded = load_layer(&path)?;

    assert_eq!(loaded.version, Some(2));
    Ok(())
}

#[test]
fn validate_config_rejects_webhook_loopback_when_enabled() {
    let mut cfg = Config::default();
    cfg.agent.webhook.enabled = Some(true);
    cfg.agent.webhook.url = Some("https://127.0.0.1/hook".to_string());

    let err = validate_config(&cfg).expect_err("expected loopback webhook URL to fail");
    assert!(err.to_string().contains("loopback") || err.to_string().contains("link-local"));
}

#[test]
fn validate_config_rejects_webhook_http_without_opt_in() {
    let mut cfg = Config::default();
    cfg.agent.webhook.enabled = Some(true);
    cfg.agent.webhook.url = Some("https://hooks.example.com/ok".to_string());
    validate_config(&cfg).expect("https public URL should validate");

    cfg.agent.webhook.url = Some("http://hooks.example.com/plain".to_string());
    let err = validate_config(&cfg).expect_err("http without opt-in should fail");
    assert!(err.to_string().contains("http://"));
}

#[test]
fn validate_config_rejects_reserved_profile_names() {
    let cfg = Config {
        profiles: Some(std::collections::BTreeMap::from([(
            "safe".to_string(),
            crate::contracts::AgentConfig::default(),
        )])),
        ..Config::default()
    };

    let err = validate_config(&cfg).expect_err("expected reserved profile name to fail");
    assert!(err.to_string().contains("reserved built-in profile name"));
}

#[test]
fn validate_config_rejects_empty_ci_gate_argv_when_enabled() {
    let mut cfg = Config::default();
    cfg.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec!["".to_string()]),
    });

    let err = validate_config(&cfg).expect_err("expected validation to fail");
    assert!(err.to_string().contains("agent.ci_gate.argv"));
}

#[test]
fn validate_config_allows_missing_ci_gate_shape_when_disabled() {
    let mut cfg = Config::default();
    cfg.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(false),
        argv: None,
    });

    validate_config(&cfg).expect("validation should pass when disabled");
}

#[test]
fn validate_config_rejects_shell_launcher_argv_without_shell_mode() {
    let mut cfg = Config::default();
    cfg.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            "make ci".to_string(),
        ]),
    });

    let err = validate_config(&cfg).expect_err("expected validation to fail");
    assert!(
        err.to_string()
            .contains("shell launcher argv is not supported")
    );
}

#[test]
fn validate_project_execution_trust_rejects_untrusted_project_ci_gate() {
    let mut layer = ConfigLayer::default();
    layer.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec!["cargo".to_string(), "test".to_string()]),
    });

    let err = validate_project_execution_trust(Some(&layer), &RepoTrust::default())
        .expect_err("expected trust failure");
    assert!(err.to_string().contains(ERR_PROJECT_EXECUTION_TRUST));
}

#[test]
fn validate_project_execution_trust_allows_trusted_project_ci_gate() {
    let mut layer = ConfigLayer::default();
    layer.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec!["cargo".to_string(), "test".to_string()]),
    });

    validate_project_execution_trust(
        Some(&layer),
        &RepoTrust {
            allow_project_commands: true,
            trusted_at: None,
        },
    )
    .expect("trusted project config should pass");
}

#[test]
fn validate_project_execution_trust_rejects_untrusted_project_plugins() {
    let mut layer = ConfigLayer::default();
    layer.plugins.plugins.insert(
        "test.plugin".to_string(),
        crate::contracts::PluginConfig {
            enabled: Some(true),
            ..Default::default()
        },
    );

    let err = validate_project_execution_trust(Some(&layer), &RepoTrust::default())
        .expect_err("expected trust failure");
    assert!(err.to_string().contains(ERR_PROJECT_EXECUTION_TRUST));
}

#[test]
fn validate_project_execution_trust_rejects_untrusted_project_runner_bin_override() {
    let mut layer = ConfigLayer::default();
    layer.agent.codex_bin = Some("/tmp/codex".to_string());

    let err = validate_project_execution_trust(Some(&layer), &RepoTrust::default())
        .expect_err("expected trust failure");
    assert!(err.to_string().contains(ERR_PROJECT_EXECUTION_TRUST));
}

#[test]
fn validate_project_execution_trust_rejects_untrusted_project_plugin_runner_selection() {
    let mut layer = ConfigLayer::default();
    layer.agent.runner = Some(crate::contracts::Runner::Plugin("test.plugin".to_string()));

    let err = validate_project_execution_trust(Some(&layer), &RepoTrust::default())
        .expect_err("expected trust failure");
    assert!(err.to_string().contains(ERR_PROJECT_EXECUTION_TRUST));
}

#[test]
fn validate_config_rejects_zero_iterations() {
    let mut cfg = Config::default();
    cfg.agent.iterations = Some(0);

    let err = validate_config(&cfg).expect_err("expected validation to fail");
    assert!(err.to_string().contains("agent.iterations"));
}

#[test]
fn validate_config_rejects_parallel_workers_lt_two() {
    let mut cfg = Config::default();
    cfg.parallel.workers = Some(1);

    let err = validate_config(&cfg).expect_err("expected validation to fail");
    assert!(err.to_string().contains("parallel.workers"));
}

// Tests for merge_retries and branch_prefix removed in direct-push rewrite
// These config keys no longer exist

#[test]
fn validate_config_rejects_zero_session_timeout_hours() {
    let mut cfg = Config::default();
    cfg.agent.session_timeout_hours = Some(0);

    let err = validate_config(&cfg).expect_err("expected validation to fail");
    assert!(err.to_string().contains("agent.session_timeout_hours"));
}

#[test]
fn validate_config_rejects_empty_cursor_bin() {
    let mut cfg = Config::default();
    cfg.agent.cursor_bin = Some("   ".to_string());

    let err = validate_config(&cfg).expect_err("expected validation to fail");
    assert!(err.to_string().contains("agent.cursor_bin"));
}

// Tests for kimi_bin and pi_bin validation (previously missing)

#[test]
fn validate_config_rejects_empty_kimi_bin() {
    let mut cfg = Config::default();
    cfg.agent.kimi_bin = Some("   ".to_string());

    let err = validate_config(&cfg).expect_err("expected validation to fail");
    assert!(err.to_string().contains("agent.kimi_bin"));
}

#[test]
fn validate_config_rejects_empty_pi_bin() {
    let mut cfg = Config::default();
    cfg.agent.pi_bin = Some("   ".to_string());

    let err = validate_config(&cfg).expect_err("expected validation to fail");
    assert!(err.to_string().contains("agent.pi_bin"));
}

#[test]
fn validate_config_accepts_valid_binary_paths() {
    let mut cfg = Config::default();
    cfg.agent.codex_bin = Some("/usr/local/bin/codex".to_string());
    cfg.agent.kimi_bin = Some("kimi".to_string());
    cfg.agent.pi_bin = Some("pi".to_string());

    validate_config(&cfg).expect("validation should pass with valid paths");
}

// Tests for validate_agent_patch binary path validation

#[test]
fn validate_agent_patch_rejects_empty_kimi_bin() {
    use super::validation::validate_agent_patch;
    use crate::contracts::AgentConfig;

    let agent = AgentConfig {
        kimi_bin: Some("   ".to_string()),
        ..Default::default()
    };

    let err = validate_agent_patch(&agent, "profiles.test").expect_err("should fail");
    assert!(err.to_string().contains("profiles.test.kimi_bin"));
}

#[test]
fn validate_agent_patch_rejects_empty_pi_bin() {
    use super::validation::validate_agent_patch;
    use crate::contracts::AgentConfig;

    let agent = AgentConfig {
        pi_bin: Some("".to_string()),
        ..Default::default()
    };

    let err = validate_agent_patch(&agent, "profiles.dev").expect_err("should fail");
    assert!(err.to_string().contains("profiles.dev.pi_bin"));
}

#[test]
fn validate_agent_patch_accepts_valid_binary_paths() {
    use super::validation::validate_agent_patch;
    use crate::contracts::AgentConfig;

    let agent = AgentConfig {
        codex_bin: Some("/usr/local/bin/codex".to_string()),
        kimi_bin: Some("kimi".to_string()),
        pi_bin: Some("pi".to_string()),
        ..Default::default()
    };

    validate_agent_patch(&agent, "profiles.valid").expect("validation should pass");
}

// Tests for instruction_files validation (validate_instruction_file_paths)

#[test]
fn validate_instruction_file_paths_rejects_missing_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut cfg = Config::default();
    cfg.agent.instruction_files = Some(vec![PathBuf::from("nonexistent.md")]);

    let err = validate_instruction_file_paths(temp.path(), &cfg).expect_err("should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent.md"),
        "Error should mention the file: {}",
        msg
    );
    assert!(
        msg.contains("read bytes from") || msg.contains("No such file"),
        "Error should indicate file not found: {}",
        msg
    );
}

#[test]
fn validate_instruction_file_paths_accepts_valid_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let file_path = temp.path().join("valid.md");
    std::fs::write(&file_path, "Valid instruction content").unwrap();

    let mut cfg = Config::default();
    cfg.agent.instruction_files = Some(vec![file_path]);

    validate_instruction_file_paths(temp.path(), &cfg).expect("should pass");
}

#[test]
fn validate_instruction_file_paths_rejects_empty_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let file_path = temp.path().join("empty.md");
    std::fs::write(&file_path, "").unwrap();

    let mut cfg = Config::default();
    cfg.agent.instruction_files = Some(vec![file_path]);

    let err = validate_instruction_file_paths(temp.path(), &cfg).expect_err("should fail");
    assert!(
        err.to_string().contains("empty"),
        "Error should indicate file is empty"
    );
}

#[test]
fn validate_instruction_file_paths_rejects_non_utf8_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let file_path = temp.path().join("invalid.md");
    // Write invalid UTF-8 bytes
    std::fs::write(&file_path, vec![0x80, 0x81, 0x82]).unwrap();

    let mut cfg = Config::default();
    cfg.agent.instruction_files = Some(vec![file_path]);

    let err = validate_instruction_file_paths(temp.path(), &cfg).expect_err("should fail");
    assert!(
        err.to_string().contains("UTF-8"),
        "Error should indicate invalid UTF-8: {}",
        err
    );
}

#[test]
fn validate_instruction_file_paths_resolves_relative_paths() {
    let temp = tempfile::TempDir::new().unwrap();
    let file_path = temp.path().join("instructions.md");
    std::fs::write(&file_path, "Content").unwrap();

    let mut cfg = Config::default();
    // Use relative path
    cfg.agent.instruction_files = Some(vec![PathBuf::from("instructions.md")]);

    validate_instruction_file_paths(temp.path(), &cfg).expect("should pass");
}

#[test]
fn validate_instruction_file_paths_resolves_absolute_paths() {
    let temp = tempfile::TempDir::new().unwrap();
    let file_path = temp.path().join("absolute.md");
    std::fs::write(&file_path, "Absolute path content").unwrap();

    let mut cfg = Config::default();
    // Use absolute path
    cfg.agent.instruction_files = Some(vec![file_path.clone()]);

    validate_instruction_file_paths(temp.path(), &cfg).expect("should pass");
}

#[test]
fn validate_instruction_file_paths_is_noop_when_none_configured() {
    let temp = tempfile::TempDir::new().unwrap();
    let cfg = Config::default();

    // Should not fail when instruction_files is None
    validate_instruction_file_paths(temp.path(), &cfg).expect("should pass with no files");
}

#[test]
fn validate_instruction_file_paths_validates_all_files_and_fails_on_first_error() {
    let temp = tempfile::TempDir::new().unwrap();

    // Create one valid file and one missing file
    let valid_path = temp.path().join("valid.md");
    std::fs::write(&valid_path, "Valid content").unwrap();

    let mut cfg = Config::default();
    cfg.agent.instruction_files = Some(vec![PathBuf::from("missing.md"), valid_path]);

    let err = validate_instruction_file_paths(temp.path(), &cfg).expect_err("should fail");
    assert!(
        err.to_string().contains("missing.md"),
        "Error should mention the first missing file"
    );
}

// Tests for queue validation consistency between validate_config and resolve_* helpers

fn assert_same_error(actual: anyhow::Error, expected: &str) {
    assert_eq!(actual.to_string(), expected);
}

#[test]
fn queue_id_prefix_error_is_consistent_between_validate_and_resolve() {
    let mut cfg = Config::default();
    cfg.queue.id_prefix = Some("   ".to_string());

    assert_same_error(
        validate_config(&cfg).unwrap_err(),
        ERR_EMPTY_QUEUE_ID_PREFIX,
    );
    assert_same_error(
        resolve_id_prefix(&cfg).unwrap_err(),
        ERR_EMPTY_QUEUE_ID_PREFIX,
    );
}

#[test]
fn queue_id_width_error_is_consistent_between_validate_and_resolve() {
    let mut cfg = Config::default();
    cfg.queue.id_width = Some(0);

    assert_same_error(
        validate_config(&cfg).unwrap_err(),
        ERR_INVALID_QUEUE_ID_WIDTH,
    );
    assert_same_error(
        resolve_id_width(&cfg).unwrap_err(),
        ERR_INVALID_QUEUE_ID_WIDTH,
    );
}

#[test]
fn queue_file_error_is_consistent_between_validate_and_resolve() {
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from(""));

    assert_same_error(validate_config(&cfg).unwrap_err(), ERR_EMPTY_QUEUE_FILE);
    assert_same_error(
        resolve_queue_path(std::path::Path::new("/repo"), &cfg).unwrap_err(),
        ERR_EMPTY_QUEUE_FILE,
    );
}

#[test]
fn queue_done_file_error_is_consistent_between_validate_and_resolve() {
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(PathBuf::from(""));

    assert_same_error(
        validate_config(&cfg).unwrap_err(),
        ERR_EMPTY_QUEUE_DONE_FILE,
    );
    assert_same_error(
        resolve_done_path(std::path::Path::new("/repo"), &cfg).unwrap_err(),
        ERR_EMPTY_QUEUE_DONE_FILE,
    );
}

// Tests for queue.aging_thresholds validation

#[test]
fn validate_config_accepts_valid_aging_thresholds() {
    use crate::contracts::QueueAgingThresholds;

    let mut cfg = Config::default();
    cfg.queue.aging_thresholds = Some(QueueAgingThresholds {
        warning_days: Some(5),
        stale_days: Some(10),
        rotten_days: Some(20),
    });

    validate_config(&cfg).expect("validation should pass with valid thresholds");
}

#[test]
fn validate_config_accepts_default_aging_thresholds() {
    let cfg = Config::default();
    // aging_thresholds is None by default
    validate_config(&cfg).expect("validation should pass with default (None) thresholds");
}

#[test]
fn validate_config_accepts_partial_aging_thresholds() {
    use crate::contracts::QueueAgingThresholds;

    let mut cfg = Config::default();
    // Only set warning_days
    cfg.queue.aging_thresholds = Some(QueueAgingThresholds {
        warning_days: Some(5),
        stale_days: None,
        rotten_days: None,
    });

    validate_config(&cfg).expect("validation should pass with partial thresholds");
}

#[test]
fn validate_config_rejects_warning_greater_than_stale() {
    use crate::contracts::QueueAgingThresholds;

    let mut cfg = Config::default();
    cfg.queue.aging_thresholds = Some(QueueAgingThresholds {
        warning_days: Some(30),
        stale_days: Some(14),
        rotten_days: Some(7),
    });

    let err = validate_config(&cfg).expect_err("should fail with reversed ordering");
    assert!(err.to_string().contains("aging_thresholds ordering"));
}

#[test]
fn validate_config_rejects_equal_warning_and_stale() {
    use crate::contracts::QueueAgingThresholds;

    let mut cfg = Config::default();
    cfg.queue.aging_thresholds = Some(QueueAgingThresholds {
        warning_days: Some(7),
        stale_days: Some(7),
        rotten_days: Some(14),
    });

    let err = validate_config(&cfg).expect_err("should fail with equal values");
    assert!(err.to_string().contains("aging_thresholds ordering"));
}

#[test]
fn validate_config_rejects_stale_greater_than_rotten() {
    use crate::contracts::QueueAgingThresholds;

    let mut cfg = Config::default();
    cfg.queue.aging_thresholds = Some(QueueAgingThresholds {
        warning_days: Some(5),
        stale_days: Some(20),
        rotten_days: Some(10),
    });

    let err = validate_config(&cfg).expect_err("should fail with stale > rotten");
    assert!(err.to_string().contains("aging_thresholds ordering"));
}

#[test]
fn validate_config_rejects_warning_greater_than_rotten_transitive() {
    use crate::contracts::QueueAgingThresholds;

    let mut cfg = Config::default();
    cfg.queue.aging_thresholds = Some(QueueAgingThresholds {
        warning_days: Some(20),
        stale_days: None, // Middle value unset, but still invalid
        rotten_days: Some(10),
    });

    let err = validate_config(&cfg).expect_err("should fail with warning > rotten");
    assert!(err.to_string().contains("aging_thresholds ordering"));
}

#[test]
fn validate_config_rejects_equal_stale_and_rotten() {
    use crate::contracts::QueueAgingThresholds;

    let mut cfg = Config::default();
    cfg.queue.aging_thresholds = Some(QueueAgingThresholds {
        warning_days: Some(5),
        stale_days: Some(14),
        rotten_days: Some(14),
    });

    let err = validate_config(&cfg).expect_err("should fail with equal stale and rotten");
    assert!(err.to_string().contains("aging_thresholds ordering"));
}
