//! Shared config and override builders for run-command tests.
//!
//! Purpose:
//! - Shared config and override builders for run-command tests.
//!
//! Responsibilities:
//! - Construct resolved config fixtures with stable queue and agent defaults.
//! - Centralize task-agent and CLI-override builders used across run suites.
//!
//! Not handled here:
//! - Queue/task fixtures.
//! - Log-capture support.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Helpers mirror the current run-command config contract closely enough for unit tests.
//! - Queue paths remain rooted under `.ralph/` for all generated resolved configs.

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Config, GitPublishMode, GitRevertMode, Model, ModelEffort,
    NotificationConfig, PhaseOverrides, QueueConfig, ReasoningEffort, Runner, RunnerRetryConfig,
    TaskAgent,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn default_queue_config() -> QueueConfig {
    QueueConfig {
        file: Some(PathBuf::from(".ralph/queue.json")),
        done_file: Some(PathBuf::from(".ralph/done.json")),
        id_prefix: Some("RQ".to_string()),
        id_width: Some(4),
        size_warning_threshold_kb: Some(500),
        task_count_warning_threshold: Some(500),
        max_dependency_depth: Some(10),
        auto_archive_terminal_after_days: None,
        aging_thresholds: None,
    }
}

fn base_agent_config() -> AgentConfig {
    AgentConfig {
        runner: None,
        model: None,
        reasoning_effort: None,
        iterations: None,
        followup_reasoning_effort: None,
        codex_bin: Some("codex".to_string()),
        opencode_bin: Some("opencode".to_string()),
        gemini_bin: Some("gemini".to_string()),
        claude_bin: Some("claude".to_string()),
        cursor_bin: Some("agent".to_string()),
        kimi_bin: Some("kimi".to_string()),
        pi_bin: Some("pi".to_string()),
        phases: Some(3),
        claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
        runner_cli: None,
        phase_overrides: None,
        instruction_files: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        ci_gate: Some(crate::contracts::CiGateConfig {
            enabled: Some(true),
            argv: Some(vec!["make".to_string(), "ci".to_string()]),
        }),
        git_revert_mode: Some(GitRevertMode::Ask),
        git_publish_mode: Some(GitPublishMode::CommitAndPush),
        notification: NotificationConfig::default(),
        webhook: crate::contracts::WebhookConfig::default(),
        runner_retry: RunnerRetryConfig::default(),
        session_timeout_hours: None,
        scan_prompt_version: None,
    }
}

fn build_resolved(repo_root: PathBuf, cfg: Config) -> config::Resolved {
    config::Resolved {
        config: cfg,
        repo_root: repo_root.clone(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
    }
}

pub(crate) fn resolved_with_agent_defaults(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: Option<ReasoningEffort>,
) -> config::Resolved {
    let dir = TempDir::new().expect("temp dir");
    let repo_root = dir.path().to_path_buf();
    let cfg = Config {
        agent: AgentConfig {
            runner,
            model,
            reasoning_effort: effort,
            phases: Some(2),
            notification: NotificationConfig {
                enabled: Some(false),
                ..NotificationConfig::default()
            },
            ..base_agent_config()
        },
        queue: default_queue_config(),
        ..Config::default()
    };
    build_resolved(repo_root, cfg)
}

pub(crate) fn resolved_with_repo_root(repo_root: PathBuf) -> config::Resolved {
    let cfg = Config {
        agent: AgentConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            reasoning_effort: Some(ReasoningEffort::Medium),
            notification: NotificationConfig {
                enabled: Some(false),
                ..NotificationConfig::default()
            },
            ..base_agent_config()
        },
        queue: default_queue_config(),
        ..Config::default()
    };
    build_resolved(repo_root, cfg)
}

pub(crate) fn resolved_with_notification_config(
    notify_on_complete: Option<bool>,
    notify_on_fail: Option<bool>,
    notify_on_loop_complete: Option<bool>,
) -> config::Resolved {
    let dir = TempDir::new().expect("temp dir");
    let repo_root = dir.path().to_path_buf();
    let cfg = Config {
        agent: AgentConfig {
            runner: Some(Runner::Claude),
            model: Some(Model::Gpt53),
            phases: Some(2),
            notification: NotificationConfig {
                enabled: Some(true),
                notify_on_complete,
                notify_on_fail,
                notify_on_loop_complete,
                notify_on_watch_new_tasks: None,
                suppress_when_active: Some(true),
                sound_enabled: Some(false),
                sound_path: None,
                timeout_ms: Some(8000),
            },
            ..base_agent_config()
        },
        queue: default_queue_config(),
        ..Config::default()
    };
    build_resolved(repo_root, cfg)
}

pub(crate) fn overrides_with_notifications(
    notify_on_complete: Option<bool>,
    notify_on_fail: Option<bool>,
) -> AgentOverrides {
    AgentOverrides {
        profile: None,
        runner: None,
        model: None,
        reasoning_effort: None,
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_publish_mode: None,
        include_draft: None,
        notify_on_complete,
        notify_on_fail,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides: None,
    }
}

pub(crate) fn test_config_agent(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: Option<ReasoningEffort>,
) -> AgentConfig {
    AgentConfig {
        runner,
        model,
        reasoning_effort: effort,
        ..base_agent_config()
    }
}

pub(crate) fn test_task_agent(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: ModelEffort,
) -> TaskAgent {
    TaskAgent {
        runner,
        model,
        model_effort: effort,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    }
}

pub(crate) fn test_overrides_with_phases(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: Option<ReasoningEffort>,
    phase_overrides: Option<PhaseOverrides>,
) -> AgentOverrides {
    AgentOverrides {
        profile: None,
        runner,
        model,
        reasoning_effort: effort,
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_publish_mode: None,
        include_draft: None,
        notify_on_complete: None,
        notify_on_fail: None,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides,
    }
}
