//! Notification configuration for post-run supervision.
//!
//! Responsibilities:
//! - Build notification configuration from resolved config and CLI overrides.
//! - CLI overrides take precedence over config file settings.
//!
//! Not handled here:
//! - Actual notification delivery (handled by crate::notification).
//! - Queue or git operations.
//!
//! Invariants/assumptions:
//! - Notification settings are optional with sensible defaults.

use crate::notification;

/// Build notification configuration from resolved config and CLI overrides.
pub(crate) fn build_notification_config(
    resolved: &crate::config::Resolved,
    notify_on_complete: Option<bool>,
    notify_sound: Option<bool>,
) -> notification::NotificationConfig {
    // CLI overrides take precedence over config
    let enabled = notify_on_complete
        .or(resolved.config.agent.notification.enabled)
        .unwrap_or(true);
    let notify_on_complete = notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = resolved
        .config
        .agent
        .notification
        .notify_on_fail
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let suppress_when_active = resolved
        .config
        .agent
        .notification
        .suppress_when_active
        .unwrap_or(true);
    let sound_enabled = notify_sound
        .or(resolved.config.agent.notification.sound_enabled)
        .unwrap_or(false);
    notification::NotificationConfig {
        enabled,
        notify_on_complete,
        notify_on_fail,
        notify_on_loop_complete,
        suppress_when_active,
        sound_enabled,
        sound_path: resolved.config.agent.notification.sound_path.clone(),
        timeout_ms: resolved
            .config
            .agent
            .notification
            .timeout_ms
            .unwrap_or(8000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        AgentConfig, Config, NotificationConfig, QueueConfig, Runner, RunnerRetryConfig,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn resolved_with_notification(
        repo_root: &std::path::Path,
        notification: NotificationConfig,
    ) -> crate::config::Resolved {
        let cfg = Config {
            agent: AgentConfig {
                runner: Some(Runner::Codex),
                model: Some(crate::contracts::Model::Gpt52Codex),
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
                claude_permission_mode: Some(
                    crate::contracts::ClaudePermissionMode::BypassPermissions,
                ),
                update_task_before_run: None,
                fail_on_prerun_update_error: None,
                runner_cli: None,
                phase_overrides: None,
                instruction_files: None,
                repoprompt_plan_required: Some(false),
                repoprompt_tool_injection: Some(false),
                ci_gate_command: Some("make ci".to_string()),
                ci_gate_enabled: Some(false),
                git_revert_mode: Some(crate::contracts::GitRevertMode::Disabled),
                git_commit_push_enabled: Some(true),
                phases: Some(2),
                notification,
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
            tui: crate::contracts::TuiConfig {
                auto_archive_terminal: None,
                celebrations_enabled: Some(false),
                stats_enabled: Some(false),
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

    #[test]
    fn build_notification_config_uses_defaults() {
        let temp = TempDir::new().unwrap();
        let notification = NotificationConfig::default();
        let resolved = resolved_with_notification(temp.path(), notification);

        let config = build_notification_config(&resolved, None, None);

        assert!(config.enabled);
        assert!(config.notify_on_complete);
        assert!(config.notify_on_fail);
        assert!(config.notify_on_loop_complete);
        assert!(config.suppress_when_active);
        assert!(!config.sound_enabled);
        assert_eq!(config.timeout_ms, 8000);
    }

    #[test]
    fn build_notification_config_cli_overrides_take_precedence() {
        let temp = TempDir::new().unwrap();
        let notification = NotificationConfig {
            enabled: Some(false),
            notify_on_complete: Some(false),
            notify_on_fail: Some(false),
            notify_on_loop_complete: Some(false),
            suppress_when_active: Some(false),
            sound_enabled: Some(false),
            sound_path: None,
            timeout_ms: Some(5000),
        };
        let resolved = resolved_with_notification(temp.path(), notification);

        // CLI overrides should take precedence
        let config = build_notification_config(&resolved, Some(true), Some(true));

        assert!(config.enabled); // CLI override
        assert!(config.notify_on_complete); // CLI override
        assert!(!config.notify_on_fail); // From config (no CLI override)
        assert!(!config.notify_on_loop_complete); // From config
        assert!(!config.suppress_when_active); // From config
        assert!(config.sound_enabled); // CLI override
        assert_eq!(config.timeout_ms, 5000); // From config
    }

    #[test]
    fn build_notification_config_respects_config_values() {
        let temp = TempDir::new().unwrap();
        let notification = NotificationConfig {
            enabled: Some(true),
            notify_on_complete: Some(false),
            notify_on_fail: Some(true),
            notify_on_loop_complete: Some(false),
            suppress_when_active: Some(true),
            sound_enabled: Some(true),
            sound_path: Some("/custom/sound.wav".to_string()),
            timeout_ms: Some(10000),
        };
        let resolved = resolved_with_notification(temp.path(), notification);

        let config = build_notification_config(&resolved, None, None);

        assert!(config.enabled);
        assert!(!config.notify_on_complete);
        assert!(config.notify_on_fail);
        assert!(!config.notify_on_loop_complete);
        assert!(config.suppress_when_active);
        assert!(config.sound_enabled);
        assert_eq!(config.sound_path, Some("/custom/sound.wav".to_string()));
        assert_eq!(config.timeout_ms, 10000);
    }
}
