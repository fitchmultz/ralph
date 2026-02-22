//! Unit tests for config.rs (config loading, merging, defaults, validation).

use ralph::config;
use ralph::contracts::{
    AgentConfig, Config, GitRevertMode, Model, NotificationConfig, ProjectType, QueueConfig,
    ReasoningEffort, Runner, RunnerRetryConfig, WebhookConfig,
};
use serial_test::serial;
use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use test_support::env_lock;

mod test_support;

// Helper to create a minimal .ralph directory
fn setup_ralph_dir(dir: &TempDir) -> PathBuf {
    let ralph_dir = dir.path().join(".ralph");
    fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    ralph_dir
}

// Helper to create a queue.json file
fn create_queue_json(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let queue_path = ralph_dir.join("queue.json");
    fs::write(&queue_path, content).expect("write queue.json");
    queue_path
}

// Helper to create a queue.jsonc file
fn create_queue_jsonc(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let queue_path = ralph_dir.join("queue.jsonc");
    fs::write(&queue_path, content).expect("write queue.jsonc");
    queue_path
}

// Helper to create a done.json file
#[allow(dead_code)]
fn create_done_json(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let done_path = ralph_dir.join("done.json");
    fs::write(&done_path, content).expect("write done.json");
    done_path
}

// Helper to create a done.jsonc file
#[allow(dead_code)]
fn create_done_jsonc(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let done_path = ralph_dir.join("done.jsonc");
    fs::write(&done_path, content).expect("write done.jsonc");
    done_path
}

// Helper to create a config.json file
fn create_config_json(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let config_path = ralph_dir.join("config.json");
    fs::write(&config_path, content).expect("write config.json");
    config_path
}

// Helper to create a config.jsonc file
fn create_config_jsonc(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let config_path = ralph_dir.join("config.jsonc");
    fs::write(&config_path, content).expect("write config.jsonc");
    config_path
}

#[test]
fn test_find_repo_root_via_ralph_queue() {
    let dir = TempDir::new().expect("create temp dir");
    create_queue_json(&dir, r#"{"version":1,"tasks":[]}"#);

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_via_ralph_config() {
    let dir = TempDir::new().expect("create temp dir");
    create_config_json(&dir, r#"{"version":1}"#);

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_via_git() {
    let dir = TempDir::new().expect("create temp dir");
    let git_dir = dir.path().join(".git");
    fs::create_dir_all(&git_dir).expect("create .git dir");

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_nested() {
    let dir = TempDir::new().expect("create temp dir");
    create_queue_json(&dir, r#"{"version":1,"tasks":[]}"#);

    let nested = dir.path().join("nested").join("deep");
    fs::create_dir_all(&nested).expect("create nested dirs");

    let repo_root = config::find_repo_root(&nested);
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_fallback_to_start() {
    let dir = test_support::temp_dir_outside_repo();
    // No .ralph or .git directory

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_project_config_path() {
    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();

    let config_path = config::project_config_path(repo_root);
    assert_eq!(config_path, repo_root.join(".ralph").join("config.jsonc"));
}

#[test]
fn test_global_config_path_xdg() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = TempDir::new().expect("create temp dir");
    let xdg_config = dir.path().join(".config");
    fs::create_dir_all(xdg_config.join("ralph")).expect("create xdg config dir");

    unsafe { env::set_var("XDG_CONFIG_HOME", &xdg_config) };
    let config_path = config::global_config_path();
    unsafe { env::remove_var("XDG_CONFIG_HOME") };

    assert!(config_path.is_some());
    assert_eq!(
        config_path.unwrap(),
        xdg_config.join("ralph").join("config.jsonc")
    );
}

#[test]
fn test_global_config_path_home() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = TempDir::new().expect("create temp dir");
    let home_config = dir.path().join(".config").join("ralph");
    fs::create_dir_all(&home_config).expect("create home config dir");

    unsafe { env::set_var("HOME", dir.path()) };
    unsafe { env::remove_var("XDG_CONFIG_HOME") };
    let config_path = config::global_config_path();
    unsafe { env::remove_var("HOME") };

    assert!(config_path.is_some());
    assert_eq!(
        config_path.unwrap(),
        dir.path()
            .join(".config")
            .join("ralph")
            .join("config.jsonc")
    );
}

#[test]
fn test_global_config_path_none_if_no_home() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { env::remove_var("XDG_CONFIG_HOME") };
    unsafe { env::remove_var("HOME") };
    let config_path = config::global_config_path();
    assert!(config_path.is_none());
}

#[test]
#[serial]
fn test_resolve_queue_path_relative() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let cfg = Config::default();

    let queue_path = config::resolve_queue_path(repo_root, &cfg).unwrap();
    assert_eq!(queue_path, repo_root.join(".ralph/queue.jsonc"));
}

#[test]
#[serial]
fn test_resolve_queue_path_custom_relative() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("custom/queue.json"));

    let queue_path = config::resolve_queue_path(repo_root, &cfg).unwrap();
    assert_eq!(queue_path, repo_root.join("custom/queue.json"));
}

#[test]
#[serial]
fn test_resolve_queue_path_absolute() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let absolute = PathBuf::from("/tmp/absolute/queue.json");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.file = Some(absolute.clone());

    let queue_path = config::resolve_queue_path(repo_root, &cfg).unwrap();
    assert_eq!(queue_path, absolute);
}

#[test]
#[serial]
fn test_resolve_queue_path_empty_fails() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from(""));

    let result = config::resolve_queue_path(repo_root, &cfg);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_resolve_done_path_relative() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let cfg = Config::default();

    let done_path = config::resolve_done_path(repo_root, &cfg).unwrap();
    assert_eq!(done_path, repo_root.join(".ralph/done.jsonc"));
}

#[test]
#[serial]
fn test_resolve_done_path_custom_relative() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(PathBuf::from("custom/done.json"));

    let done_path = config::resolve_done_path(repo_root, &cfg).unwrap();
    assert_eq!(done_path, repo_root.join("custom/done.json"));
}

#[test]
#[serial]
fn test_resolve_done_path_absolute() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let absolute = PathBuf::from("/tmp/absolute/done.json");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(absolute.clone());

    let done_path = config::resolve_done_path(repo_root, &cfg).unwrap();
    assert_eq!(done_path, absolute);
}

#[test]
#[serial]
fn test_resolve_done_path_empty_fails() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(PathBuf::from(""));

    let result = config::resolve_done_path(repo_root, &cfg);
    assert!(result.is_err());
}

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
        version: 2,
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

#[test]
fn test_load_layer_valid_json() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.json");
    fs::write(
        &config_path,
        r#"{"version":1,"project_type":"code","queue":{},"agent":{}}"#,
    )
    .expect("write config");

    let layer = config::load_layer(&config_path).unwrap();
    assert_eq!(layer.version, Some(1));
    assert_eq!(layer.project_type, Some(ProjectType::Code));
}

#[test]
fn test_load_layer_parses_git_commit_push_enabled() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.json");
    fs::write(
        &config_path,
        r#"{"version":1,"agent":{"git_commit_push_enabled":false}}"#,
    )
    .expect("write config");

    let layer = config::load_layer(&config_path).unwrap();
    assert_eq!(layer.agent.git_commit_push_enabled, Some(false));
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
    let layer_json = r#"{"version":1,"queue":{},"agent":{}}"#;
    let layer = serde_json::from_str(layer_json).unwrap();

    let result = config::apply_layer(base, layer).unwrap();
    assert_eq!(result.version, 1);
}

#[test]
fn test_apply_layer_unsupported_version_fails() {
    let base = Config::default();
    let layer_json = r#"{"version":2,"queue":{},"agent":{}}"#;
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
    base.agent.model = Some(Model::Gpt52Codex);

    let layer_json = r#"{"queue":{},"agent":{"runner":"opencode"}}"#;
    let layer = serde_json::from_str(layer_json).unwrap();

    let result = config::apply_layer(base, layer).unwrap();
    assert_eq!(result.agent.runner, Some(Runner::Opencode));
    assert_eq!(result.agent.model, Some(Model::Gpt52Codex)); // preserved
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
        model: Some(Model::Gpt52Codex),
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
        ci_gate_command: Some("make ci".to_string()),
        ci_gate_enabled: Some(true),
        git_revert_mode: Some(GitRevertMode::Ask),
        git_commit_push_enabled: Some(true),
        notification: NotificationConfig::default(),
        webhook: WebhookConfig::default(),
        runner_retry: RunnerRetryConfig::default(),
        session_timeout_hours: None,
        scan_prompt_version: None,
    };

    let override_config = AgentConfig {
        runner: None,
        model: Some(Model::Gpt52),
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
        ci_gate_command: Some("custom ci".to_string()),
        ci_gate_enabled: Some(false),
        git_revert_mode: Some(GitRevertMode::Disabled),
        git_commit_push_enabled: Some(false),
        notification: NotificationConfig::default(),
        webhook: WebhookConfig::default(),
        runner_retry: RunnerRetryConfig::default(),
        session_timeout_hours: Some(48),
        scan_prompt_version: None,
    };

    base.merge_from(override_config);

    assert_eq!(base.runner, Some(Runner::Codex));
    assert_eq!(base.model, Some(Model::Gpt52));
    assert_eq!(base.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(base.iterations, Some(2));
    assert_eq!(base.followup_reasoning_effort, Some(ReasoningEffort::Low));
    assert_eq!(base.codex_bin, Some("codex".to_string()));
    assert_eq!(base.opencode_bin, Some("opencode".to_string()));
    assert_eq!(base.phases, Some(3));
    assert_eq!(base.ci_gate_command, Some("custom ci".to_string()));
    assert_eq!(base.ci_gate_enabled, Some(false));
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

#[test]
#[serial]
fn test_resolve_queue_path_expands_tilde_to_home() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let repo_root = PathBuf::from("/repo/root");
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("~/myqueue.json"));

    let queue_path = config::resolve_queue_path(&repo_root, &cfg).unwrap();
    assert_eq!(queue_path, PathBuf::from("/custom/home/myqueue.json"));

    // Restore HOME
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

// Tests for .jsonc file format support (RQ-0807)

#[test]
fn test_find_repo_root_via_ralph_queue_jsonc() {
    let dir = TempDir::new().expect("create temp dir");
    create_queue_jsonc(&dir, r#"{"version":1,"tasks":[]}"#);

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_via_ralph_config_jsonc() {
    let dir = TempDir::new().expect("create temp dir");
    create_config_jsonc(&dir, r#"{"version":1}"#);

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
#[serial]
fn test_resolve_queue_path_prefers_jsonc_over_json() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);

    // Create both .json and .jsonc files
    fs::write(ralph_dir.join("queue.json"), r#"{"version":1,"tasks":[]}"#).unwrap();
    fs::write(ralph_dir.join("queue.jsonc"), r#"{"version":1,"tasks":[]}"#).unwrap();

    // Use explicit config with the new default path to ensure is_default=true
    let cfg = Config {
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.jsonc")),
            ..Default::default()
        },
        ..Config::default()
    };
    let queue_path = config::resolve_queue_path(dir.path(), &cfg).unwrap();

    // Should prefer .jsonc over .json
    assert_eq!(queue_path, ralph_dir.join("queue.jsonc"));
}

#[test]
#[serial]
fn test_resolve_queue_path_falls_back_to_json() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);

    // Create only .json file
    fs::write(ralph_dir.join("queue.json"), r#"{"version":1,"tasks":[]}"#).unwrap();

    let cfg = Config::default();
    let queue_path = config::resolve_queue_path(dir.path(), &cfg).unwrap();

    // Should fall back to .json when .jsonc doesn't exist
    assert_eq!(queue_path, ralph_dir.join("queue.json"));
}

#[test]
#[serial]
fn test_resolve_done_path_prefers_jsonc_over_json() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);

    // Create both .json and .jsonc files at the default paths
    // The default is now .jsonc, so we need to create both files to test preference
    fs::write(ralph_dir.join("done.jsonc"), r#"{"version":1,"tasks":[]}"#).unwrap();
    fs::write(ralph_dir.join("done.json"), r#"{"version":1,"tasks":[]}"#).unwrap();

    // Use explicit config with the new default path to ensure is_default=true
    let cfg = Config {
        queue: QueueConfig {
            done_file: Some(PathBuf::from(".ralph/done.jsonc")),
            ..Default::default()
        },
        ..Config::default()
    };
    let done_path = config::resolve_done_path(dir.path(), &cfg).unwrap();

    // Should prefer .jsonc over .json
    assert_eq!(done_path, ralph_dir.join("done.jsonc"));
}

#[test]
#[serial]
fn test_resolve_done_path_falls_back_to_json() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);

    // Create only .json file
    fs::write(ralph_dir.join("done.json"), r#"{"version":1,"tasks":[]}"#).unwrap();

    let cfg = Config::default();
    let done_path = config::resolve_done_path(dir.path(), &cfg).unwrap();

    // Should fall back to .json when .jsonc doesn't exist
    assert_eq!(done_path, ralph_dir.join("done.json"));
}

#[test]
fn test_project_config_path_prefers_jsonc_over_json() {
    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);

    // Create both .json and .jsonc files
    fs::write(ralph_dir.join("config.json"), r#"{"version":1}"#).unwrap();
    fs::write(ralph_dir.join("config.jsonc"), r#"{"version":1}"#).unwrap();

    let config_path = config::project_config_path(dir.path());

    // Should prefer .jsonc over .json
    assert_eq!(config_path, ralph_dir.join("config.jsonc"));
}

#[test]
fn test_project_config_path_falls_back_to_jsonc() {
    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);

    // Create only .jsonc file
    fs::write(ralph_dir.join("config.jsonc"), r#"{"version":1}"#).unwrap();

    let config_path = config::project_config_path(dir.path());

    // Should fall back to .jsonc when .json doesn't exist
    assert_eq!(config_path, ralph_dir.join("config.jsonc"));
}

#[test]
#[serial]
fn test_global_config_path_falls_back_to_jsonc() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = TempDir::new().expect("create temp dir");
    let xdg_config = dir.path().join(".config");
    let ralph_dir = xdg_config.join("ralph");
    fs::create_dir_all(&ralph_dir).expect("create xdg config dir");

    // Create only config.jsonc (no config.json)
    fs::write(ralph_dir.join("config.jsonc"), r#"{"version":1}"#).unwrap();

    unsafe { env::set_var("XDG_CONFIG_HOME", &xdg_config) };
    let config_path = config::global_config_path();
    unsafe { env::remove_var("XDG_CONFIG_HOME") };

    assert!(config_path.is_some());
    assert_eq!(config_path.unwrap(), ralph_dir.join("config.jsonc"));
}

#[test]
#[serial]
fn test_global_config_path_prefers_jsonc_over_json() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = TempDir::new().expect("create temp dir");
    let xdg_config = dir.path().join(".config");
    let ralph_dir = xdg_config.join("ralph");
    fs::create_dir_all(&ralph_dir).expect("create xdg config dir");

    // Create both .json and .jsonc files
    fs::write(ralph_dir.join("config.json"), r#"{"version":1}"#).unwrap();
    fs::write(ralph_dir.join("config.jsonc"), r#"{"version":1}"#).unwrap();

    unsafe { env::set_var("XDG_CONFIG_HOME", &xdg_config) };
    let config_path = config::global_config_path();
    unsafe { env::remove_var("XDG_CONFIG_HOME") };

    assert!(config_path.is_some());
    // Should prefer .jsonc over .json
    assert_eq!(config_path.unwrap(), ralph_dir.join("config.jsonc"));
}

#[test]
fn test_load_layer_accepts_jsonc_with_comments() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.jsonc");

    // Write JSONC with comments
    let jsonc_content = r#"{
        // This is a single-line comment
        "version": 1,
        "agent": {
            /* Multi-line
               comment */
            "runner": "claude"
        }
    }"#;
    fs::write(&config_path, jsonc_content).expect("write config.jsonc");

    let layer = config::load_layer(&config_path).unwrap();
    assert_eq!(layer.version, Some(1));
    assert_eq!(layer.agent.runner, Some(Runner::Claude));
}

#[test]
fn test_load_queue_accepts_jsonc_with_comments() {
    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);
    let queue_path = ralph_dir.join("queue.jsonc");

    // Write JSONC with comments
    let jsonc_content = r#"{
        // Queue file with comments
        "version": 1,
        "tasks": [
            /* Task entry */
            {
                "id": "RQ-0001",
                "title": "Test task",
                "status": "todo",
                "tags": [],
                "scope": [],
                "evidence": [],
                "plan": [],
                "created_at": "2026-01-18T00:00:00Z",
                "updated_at": "2026-01-18T00:00:00Z"
            }
        ]
    }"#;
    fs::write(&queue_path, jsonc_content).expect("write queue.jsonc");

    let queue = ralph::queue::load_queue(&queue_path).unwrap();
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0001");
}

#[test]
#[serial]
fn test_resolve_done_path_expands_tilde_to_home() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let repo_root = PathBuf::from("/repo/root");
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(PathBuf::from("~/mydone.json"));

    let done_path = config::resolve_done_path(&repo_root, &cfg).unwrap();
    assert_eq!(done_path, PathBuf::from("/custom/home/mydone.json"));

    // Restore HOME
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn test_resolve_queue_path_expands_tilde_alone_to_home() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let repo_root = PathBuf::from("/repo/root");
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("~"));

    let queue_path = config::resolve_queue_path(&repo_root, &cfg).unwrap();
    assert_eq!(queue_path, PathBuf::from("/custom/home"));

    // Restore HOME
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn test_resolve_queue_path_does_not_join_when_tilde_expands() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    // When ~ expands to an absolute path, it should NOT be joined to repo_root
    let repo_root = PathBuf::from("/repo/root");
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("~/queue.json"));

    let queue_path = config::resolve_queue_path(&repo_root, &cfg).unwrap();
    // Should be /custom/home/queue.json, NOT /repo/root/custom/home/queue.json
    assert_eq!(queue_path, PathBuf::from("/custom/home/queue.json"));
    assert!(!queue_path.to_string_lossy().contains("/repo/root"));

    // Restore HOME
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn test_resolve_queue_path_relative_when_home_unset() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    // Remove HOME - tilde should not expand, path treated as relative
    unsafe { env::remove_var("HOME") };

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("~/queue.json"));

    // When HOME is unset, ~/queue.json is treated as a relative path
    let queue_path = config::resolve_queue_path(repo_root, &cfg).unwrap();
    assert_eq!(queue_path, repo_root.join("~/queue.json"));

    // Restore HOME
    if let Some(v) = original_home {
        unsafe { env::set_var("HOME", v) }
    }
}
