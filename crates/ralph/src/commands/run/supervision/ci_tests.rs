//! CI supervision tests extracted from the production module.
//!
//! Purpose:
//! - CI supervision tests extracted from the production module.
//!
//! Responsibilities:
//! - Cover CI pattern detection, compliance messaging, and continue-session behavior.
//! - Keep large scenario suites out of `ci.rs`.
//!
//! Non-scope:
//! - Production CI execution logic.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;
use crate::contracts::{
    AgentConfig, CiGateConfig, Config, NotificationConfig, QueueConfig, Runner, RunnerRetryConfig,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

fn write_repo_trust(repo_root: &std::path::Path) {
    let ralph_dir = repo_root.join(".ralph");
    fs::create_dir_all(&ralph_dir).unwrap();
    fs::write(
        ralph_dir.join("trust.jsonc"),
        r#"{
  "allow_project_commands": true,
  "trusted_at": "2026-03-07T00:00:00Z"
}"#,
    )
    .unwrap();
}

fn resolved_with_ci_command(
    repo_root: &std::path::Path,
    command: Option<String>,
    enabled: bool,
) -> crate::config::Resolved {
    let argv = command.map(|command| {
        let script_name = if cfg!(windows) {
            "ci-gate-test.cmd"
        } else {
            "ci-gate-test.sh"
        };
        let script_path = repo_root.join(script_name);
        let script = if cfg!(windows) {
            format!("@echo off\r\n{command}\r\n")
        } else {
            format!("#!/bin/sh\nset -e\n{command}\n")
        };
        fs::write(&script_path, script).expect("write CI gate test script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)
                .expect("script metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).expect("set script permissions");
        }
        vec![script_path.to_string_lossy().to_string()]
    });

    let cfg = Config {
        agent: AgentConfig {
            runner: Some(Runner::Codex),
            model: Some(crate::contracts::Model::Gpt53Codex),
            reasoning_effort: Some(crate::contracts::ReasoningEffort::Medium),
            iterations: Some(1),
            followup_reasoning_effort: None,
            codex_bin: Some("codex".to_string()),
            opencode_bin: Some("opencode".to_string()),
            gemini_bin: Some("gemini".to_string()),
            claude_bin: Some("claude".to_string()),
            cursor_bin: Some("agent".to_string()),
            kimi_bin: Some("kimi".to_string()),
            pi_bin: Some("pi".to_string()),
            claude_permission_mode: Some(crate::contracts::ClaudePermissionMode::BypassPermissions),
            runner_cli: None,
            phase_overrides: None,
            instruction_files: None,
            repoprompt_plan_required: Some(false),
            repoprompt_tool_injection: Some(false),
            ci_gate: Some(CiGateConfig {
                enabled: Some(enabled),
                argv: argv.or_else(|| Some(vec!["make".to_string(), "ci".to_string()])),
            }),
            git_revert_mode: Some(crate::contracts::GitRevertMode::Disabled),
            git_publish_mode: Some(crate::contracts::GitPublishMode::CommitAndPush),
            phases: Some(2),
            notification: NotificationConfig {
                enabled: Some(false),
                ..NotificationConfig::default()
            },
            webhook: crate::contracts::WebhookConfig::default(),
            runner_retry: RunnerRetryConfig::default(),
            session_timeout_hours: None,
            scan_prompt_version: None,
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
            auto_archive_terminal_after_days: None,
            aging_thresholds: None,
        },
        ..Config::default()
    };

    crate::config::Resolved {
        config: cfg,
        repo_root: repo_root.to_path_buf(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
    }
}

#[path = "ci_tests/continue_session.rs"]
mod continue_session;
#[path = "ci_tests/failure.rs"]
mod failure;
#[path = "ci_tests/formatting.rs"]
mod formatting;
#[path = "ci_tests/pattern_helpers.rs"]
mod pattern_helpers;
#[path = "ci_tests/pattern_keys.rs"]
mod pattern_keys;
#[path = "ci_tests/pattern_precedence.rs"]
mod pattern_precedence;
