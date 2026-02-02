//! CI gate execution for post-run supervision.
//!
//! Responsibilities:
//! - Execute the configured CI gate command (default: make ci).
//! - Provide command label for error messages.
//!
//! Not handled here:
//! - Queue maintenance (see queue_ops.rs).
//! - Git operations (see git_ops.rs).
//!
//! Invariants/assumptions:
//! - CI gate command is configured or defaults to "make ci".
//! - Command execution inherits stdin/stdout/stderr.

use super::logging;
use crate::runutil;
use anyhow::{Context, Result, bail};
use std::process::Stdio;

/// Executes the CI gate command if enabled.
pub(crate) fn run_ci_gate(resolved: &crate::config::Resolved) -> Result<()> {
    let enabled = resolved.config.agent.ci_gate_enabled.unwrap_or(true);
    let command = resolved
        .config
        .agent
        .ci_gate_command
        .as_deref()
        .unwrap_or("make ci")
        .trim();

    if !enabled {
        log::info!("CI gate disabled; skipping configured command '{command}'.");
        return Ok(());
    }

    if command.is_empty() {
        bail!(
            "CI gate command is empty but CI gate is enabled. Set agent.ci_gate_command or disable the gate with agent.ci_gate_enabled=false."
        );
    }

    logging::with_scope(&format!("CI gate ({command})"), || {
        let status = runutil::shell_command(command)
            .current_dir(&resolved.repo_root)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| {
                format!(
                    "run CI gate command '{}' in {}",
                    command,
                    resolved.repo_root.display()
                )
            })?;

        if status.success() {
            return Ok(());
        }

        bail!(
            "CI failed: '{}' exited with code {:?}. Fix the linting, type-checking, or test failures before proceeding.",
            command,
            status.code()
        )
    })
}

/// Returns the CI gate command label for display purposes.
pub(crate) fn ci_gate_command_label(resolved: &crate::config::Resolved) -> String {
    resolved
        .config
        .agent
        .ci_gate_command
        .as_deref()
        .unwrap_or("make ci")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{AgentConfig, Config, NotificationConfig, QueueConfig, Runner};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn resolved_with_ci_command(
        repo_root: &std::path::Path,
        command: Option<String>,
        enabled: bool,
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
                ci_gate_command: command,
                ci_gate_enabled: Some(enabled),
                git_revert_mode: Some(crate::contracts::GitRevertMode::Disabled),
                git_commit_push_enabled: Some(true),
                phases: Some(2),
                notification: NotificationConfig {
                    enabled: Some(false),
                    ..NotificationConfig::default()
                },
                webhook: crate::contracts::WebhookConfig::default(),
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
    fn ci_gate_command_label_returns_default() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), None, true);
        assert_eq!(ci_gate_command_label(&resolved), "make ci");
    }

    #[test]
    fn ci_gate_command_label_returns_custom() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), Some("cargo test".to_string()), true);
        assert_eq!(ci_gate_command_label(&resolved), "cargo test");
    }

    #[test]
    fn run_ci_gate_skips_when_disabled() -> Result<()> {
        let temp = TempDir::new()?;
        let resolved = resolved_with_ci_command(temp.path(), Some("make ci".to_string()), false);
        // Should succeed without running anything
        run_ci_gate(&resolved)?;
        Ok(())
    }

    #[test]
    fn run_ci_gate_errors_on_empty_command() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), Some("".to_string()), true);
        let err = run_ci_gate(&resolved).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }
}
