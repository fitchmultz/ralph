//! Config layer loading and merge tests.
//!
//! Purpose:
//! - Config layer loading and merge tests.
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
use ralph::contracts::GitPublishMode;

#[test]
fn test_load_layer_valid_json() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.json");
    fs::write(
        &config_path,
        r#"{"version":2,"project_type":"code","queue":{},"agent":{}}"#,
    )
    .expect("write config");

    let layer = config::load_layer(&config_path).unwrap();
    assert_eq!(layer.version, Some(2));
    assert_eq!(layer.project_type, Some(ProjectType::Code));
}

#[test]
fn test_load_layer_parses_git_publish_mode() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.json");
    fs::write(
        &config_path,
        r#"{"version":2,"agent":{"git_publish_mode":"commit"}}"#,
    )
    .expect("write config");

    let layer = config::load_layer(&config_path).unwrap();
    assert_eq!(layer.agent.git_publish_mode, Some(GitPublishMode::Commit));
}

#[test]
fn test_load_layer_invalid_json_fails() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.json");
    fs::write(&config_path, "{invalid json}").expect("write config");

    let result = config::load_layer(&config_path);
    assert!(result.is_err());
}

#[test]
fn test_load_layer_missing_file_fails() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("nonexistent.json");

    let result = config::load_layer(&config_path);
    assert!(result.is_err());
}

#[test]
fn test_apply_layer_version() {
    let base = Config::default();
    let layer_json = r#"{"version":2,"queue":{},"agent":{}}"#;
    let layer = serde_json::from_str(layer_json).unwrap();

    let result = config::apply_layer(base, layer).unwrap();
    assert_eq!(result.version, 2);
}

#[test]
fn test_apply_layer_unsupported_version_fails() {
    let base = Config::default();
    let layer_json = r#"{"version":1,"queue":{},"agent":{}}"#;
    let layer = serde_json::from_str(layer_json).unwrap();

    let result = config::apply_layer(base, layer);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Unsupported config version"));
}

#[test]
fn test_apply_layer_project_type() {
    let base = Config {
        project_type: Some(ProjectType::Code),
        ..Default::default()
    };
    let layer_json = r#"{"project_type":"docs","queue":{},"agent":{}}"#;
    let layer = serde_json::from_str(layer_json).unwrap();

    let result = config::apply_layer(base, layer).unwrap();
    assert_eq!(result.project_type, Some(ProjectType::Docs));
}

#[test]
fn test_apply_layer_merges_queue_config() {
    let mut base = Config::default();
    base.queue.id_prefix = Some("BASE".to_string());
    base.queue.id_width = Some(4);

    let layer_json = r#"{"queue":{"id_prefix":"OVERRIDE"},"agent":{}}"#;
    let layer = serde_json::from_str(layer_json).unwrap();

    let result = config::apply_layer(base, layer).unwrap();
    assert_eq!(result.queue.id_prefix, Some("OVERRIDE".to_string()));
    assert_eq!(result.queue.id_width, Some(4)); // preserved
}

#[test]
fn test_apply_layer_merges_agent_config() {
    let mut base = Config::default();
    base.agent.runner = Some(Runner::Codex);
    base.agent.model = Some(Model::Gpt53Codex);

    let layer_json = r#"{"queue":{},"agent":{"runner":"opencode"}}"#;
    let layer = serde_json::from_str(layer_json).unwrap();

    let result = config::apply_layer(base, layer).unwrap();
    assert_eq!(result.agent.runner, Some(Runner::Opencode));
    assert_eq!(result.agent.model, Some(Model::Gpt53Codex)); // preserved
}

#[test]
fn test_queue_config_merge_from_partial() {
    let mut base = QueueConfig {
        file: Some(PathBuf::from("base.json")),
        done_file: Some(PathBuf::from("base-done.json")),
        id_prefix: Some("BASE".to_string()),
        id_width: Some(4),
        size_warning_threshold_kb: Some(500),
        task_count_warning_threshold: Some(500),
        max_dependency_depth: Some(10),
        auto_archive_terminal_after_days: None,
        aging_thresholds: None,
    };

    let override_config = QueueConfig {
        file: Some(PathBuf::from("override.json")),
        done_file: None,
        id_prefix: None,
        id_width: None,
        size_warning_threshold_kb: None,
        task_count_warning_threshold: None,
        max_dependency_depth: None,
        auto_archive_terminal_after_days: None,
        aging_thresholds: None,
    };

    base.merge_from(override_config);

    assert_eq!(base.file, Some(PathBuf::from("override.json")));
    assert_eq!(base.done_file, Some(PathBuf::from("base-done.json")));
    assert_eq!(base.id_prefix, Some("BASE".to_string()));
    assert_eq!(base.id_width, Some(4));
}

#[test]
fn test_agent_config_merge_from_partial() {
    let mut base = AgentConfig {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt53Codex),
        reasoning_effort: None,
        iterations: Some(1),
        followup_reasoning_effort: None,
        codex_bin: Some("codex".to_string()),
        opencode_bin: None,
        gemini_bin: None,
        claude_bin: None,
        cursor_bin: None,
        kimi_bin: None,
        pi_bin: None,
        phases: Some(2),
        claude_permission_mode: None,
        runner_cli: None,
        phase_overrides: None,
        instruction_files: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        ci_gate: Some(CiGateConfig {
            enabled: Some(true),
            argv: Some(vec!["make".to_string(), "ci".to_string()]),
        }),
        git_revert_mode: Some(GitRevertMode::Ask),
        git_publish_mode: Some(GitPublishMode::CommitAndPush),
        notification: NotificationConfig::default(),
        webhook: WebhookConfig::default(),
        runner_retry: RunnerRetryConfig::default(),
        session_timeout_hours: None,
        scan_prompt_version: None,
    };

    let override_config = AgentConfig {
        runner: None,
        model: Some(Model::Gpt53),
        reasoning_effort: Some(ReasoningEffort::High),
        iterations: Some(2),
        followup_reasoning_effort: Some(ReasoningEffort::Low),
        codex_bin: None,
        opencode_bin: Some("opencode".to_string()),
        gemini_bin: None,
        claude_bin: None,
        cursor_bin: None,
        kimi_bin: None,
        pi_bin: None,
        phases: Some(3),
        claude_permission_mode: None,
        runner_cli: None,
        phase_overrides: None,
        instruction_files: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        ci_gate: Some(CiGateConfig {
            enabled: Some(false),
            argv: Some(vec!["custom".to_string(), "ci".to_string()]),
        }),
        git_revert_mode: Some(GitRevertMode::Disabled),
        git_publish_mode: Some(GitPublishMode::Off),
        notification: NotificationConfig::default(),
        webhook: WebhookConfig::default(),
        runner_retry: RunnerRetryConfig::default(),
        session_timeout_hours: Some(48),
        scan_prompt_version: None,
    };

    base.merge_from(override_config);

    assert_eq!(base.runner, Some(Runner::Codex));
    assert_eq!(base.model, Some(Model::Gpt53));
    assert_eq!(base.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(base.iterations, Some(2));
    assert_eq!(base.followup_reasoning_effort, Some(ReasoningEffort::Low));
    assert_eq!(base.codex_bin, Some("codex".to_string()));
    assert_eq!(base.opencode_bin, Some("opencode".to_string()));
    assert_eq!(base.phases, Some(3));
    assert_eq!(
        base.ci_gate,
        Some(CiGateConfig {
            enabled: Some(false),
            argv: Some(vec!["custom".to_string(), "ci".to_string()]),
        })
    );
}

#[test]
fn test_validate_config_invalid_phases_fails() {
    let mut cfg = Config::default();

    // Test phase 0
    cfg.agent.phases = Some(0);
    let result = config::validate_config(&cfg);
    assert!(result.is_err());

    // Test phase 4
    cfg.agent.phases = Some(4);
    let result = config::validate_config(&cfg);
    assert!(result.is_err());
}

// Tests for tilde expansion in path resolution
