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
use crate::constants::limits::CI_GATE_AUTO_RETRY_LIMIT;
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
            .env_remove(crate::config::QUEUE_PATH_OVERRIDE_ENV)
            .env_remove(crate::config::DONE_PATH_OVERRIDE_ENV)
            .env_remove(crate::config::REPO_ROOT_OVERRIDE_ENV)
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
            "CI failed: '{}' exited with code {:?}. Run '{}' again to identify the issues and fix.",
            command,
            status.code(),
            command
        )
    })
}

fn strict_ci_gate_compliance_message(resolved: &crate::config::Resolved) -> String {
    let cmd = ci_gate_command_label(resolved);
    format!(
        r#"CI gate ({}): error: CI failed: '{}' exited with an error code. Run '{}' again to identify the issues and fix. You MUST see the CI gate pass before this turn can end and proceed further. NO skipping tests, half-assed patches, or sloppy shortcuts. Flaky tests should be investigated and patched. Failures unrelated to your work are in scope and your responsibility. Implement fixes your mother would be proud of."#,
        cmd, cmd, cmd
    )
}

/// Executes CI gate with auto-retry and Continue support via a runner session.
pub(crate) fn run_ci_gate_with_continue_session<F>(
    resolved: &crate::config::Resolved,
    git_revert_mode: crate::contracts::GitRevertMode,
    revert_prompt: Option<&runutil::RevertPromptHandler>,
    continue_session: &mut super::ContinueSession,
    mut on_resume: F,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<()>
where
    F: FnMut(&crate::runner::RunnerOutput, std::time::Duration) -> Result<()>,
{
    loop {
        match run_ci_gate(resolved) {
            Ok(()) => break,
            Err(err) => {
                if continue_session.ci_failure_retry_count < CI_GATE_AUTO_RETRY_LIMIT {
                    continue_session.ci_failure_retry_count =
                        continue_session.ci_failure_retry_count.saturating_add(1);
                    let attempt = continue_session.ci_failure_retry_count;

                    log::warn!(
                        "CI gate failed; auto-sending strict compliance Continue message to agent (attempt {}/{})",
                        attempt,
                        CI_GATE_AUTO_RETRY_LIMIT
                    );

                    let message = strict_ci_gate_compliance_message(resolved);
                    let (output, elapsed) = super::resume_continue_session(
                        resolved,
                        continue_session,
                        &message,
                        plugins,
                    )?;
                    on_resume(&output, elapsed)?;
                    continue;
                }

                let outcome = runutil::apply_git_revert_mode(
                    &resolved.repo_root,
                    git_revert_mode,
                    "CI failure",
                    revert_prompt,
                )?;

                match outcome {
                    runutil::RevertOutcome::Continue { message } => {
                        let (output, elapsed) = super::resume_continue_session(
                            resolved,
                            continue_session,
                            &message,
                            plugins,
                        )?;
                        on_resume(&output, elapsed)?;
                        continue;
                    }
                    _ => {
                        bail!(
                            "{} Error: {:#}",
                            runutil::format_revert_failure_message(
                                "CI gate failed after changes. Fix issues reported by CI and rerun.",
                                outcome,
                            ),
                            err
                        );
                    }
                }
            }
        }
    }
    Ok(())
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
    use crate::contracts::{
        AgentConfig, Config, NotificationConfig, QueueConfig, Runner, RunnerRetryConfig,
    };
    use serial_test::serial;
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

    #[test]
    #[serial]
    fn run_ci_gate_strips_all_ralph_override_env() -> Result<()> {
        let prior_queue = std::env::var_os(crate::config::QUEUE_PATH_OVERRIDE_ENV);
        let prior_done = std::env::var_os(crate::config::DONE_PATH_OVERRIDE_ENV);
        let prior_repo = std::env::var_os(crate::config::REPO_ROOT_OVERRIDE_ENV);

        // SAFETY: this test is serial and restores process env before returning.
        unsafe {
            std::env::set_var(
                crate::config::QUEUE_PATH_OVERRIDE_ENV,
                "/tmp/source-queue.json",
            );
            std::env::set_var(
                crate::config::DONE_PATH_OVERRIDE_ENV,
                "/tmp/source-done.json",
            );
            std::env::set_var(crate::config::REPO_ROOT_OVERRIDE_ENV, "/tmp/workspace-root");
        }

        let temp = TempDir::new()?;
        // Verify all three Ralph environment overrides are stripped from child process
        let command = if cfg!(windows) {
            "powershell -NoProfile -Command \"if ($env:RALPH_QUEUE_PATH_OVERRIDE -or $env:RALPH_DONE_PATH_OVERRIDE -or $env:RALPH_REPO_ROOT_OVERRIDE) { exit 42 }\""
        } else {
            "sh -c 'test -z \"$RALPH_QUEUE_PATH_OVERRIDE\" && test -z \"$RALPH_DONE_PATH_OVERRIDE\" && test -z \"$RALPH_REPO_ROOT_OVERRIDE\"'"
        };
        let resolved = resolved_with_ci_command(temp.path(), Some(command.to_string()), true);
        let result = run_ci_gate(&resolved);

        // SAFETY: restore env to pre-test values.
        unsafe {
            match prior_queue {
                Some(v) => std::env::set_var(crate::config::QUEUE_PATH_OVERRIDE_ENV, v),
                None => std::env::remove_var(crate::config::QUEUE_PATH_OVERRIDE_ENV),
            }
            match prior_done {
                Some(v) => std::env::set_var(crate::config::DONE_PATH_OVERRIDE_ENV, v),
                None => std::env::remove_var(crate::config::DONE_PATH_OVERRIDE_ENV),
            }
            match prior_repo {
                Some(v) => std::env::set_var(crate::config::REPO_ROOT_OVERRIDE_ENV, v),
                None => std::env::remove_var(crate::config::REPO_ROOT_OVERRIDE_ENV),
            }
        }

        result
    }
}
