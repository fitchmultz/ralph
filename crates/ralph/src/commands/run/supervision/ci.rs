//! CI gate execution for post-run supervision.
//!
//! Responsibilities:
//! - Execute the configured CI gate command (default: make ci).
//! - Capture stdout/stderr for compliance messages.
//! - Provide command label for error messages.
//!
//! Not handled here:
//! - Queue maintenance (see queue_ops.rs).
//! - Git operations (see git_ops.rs).
//!
//! Invariants/assumptions:
//! - CI gate command is configured or defaults to "make ci".
//! - Command output is captured (not inherited) to include in compliance messages.

use super::logging;
use crate::constants::limits::CI_GATE_AUTO_RETRY_LIMIT;
use crate::runutil;
use anyhow::{Context, Result, bail};
use std::process::Stdio;

/// Result of running the CI gate command.
#[derive(Debug)]
pub(crate) struct CiGateResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Truncate a string for logging, showing the end (most recent output).
///
/// Uses character-aware truncation to avoid splitting multi-byte UTF-8 sequences.
fn truncate_for_log(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        // Skip characters from the start to show most recent output
        let skip = char_count.saturating_sub(max_chars);
        let truncated: String = s.chars().skip(skip).collect();
        format!("...{truncated}")
    }
}

/// Format CI output for inclusion in compliance message.
///
/// Takes last N lines (default 100) to show most relevant output.
/// stderr is included first since errors typically appear there.
fn format_ci_output_for_message(stdout: &str, stderr: &str, max_lines: usize) -> String {
    let mut lines: Vec<&str> = Vec::new();

    // Include stderr first (usually contains errors)
    lines.extend(stderr.lines());
    lines.extend(stdout.lines());

    // Take last N lines to show most recent/relevant output
    let start = lines.len().saturating_sub(max_lines);
    let selected = &lines[start..];

    if selected.is_empty() {
        "No output captured.".to_string()
    } else {
        format!(
            "Last {} lines of CI output:\n```\n{}\n```",
            selected.len(),
            selected.join("\n")
        )
    }
}

/// Executes the CI gate command if enabled.
///
/// Returns a CiGateResult containing success status, exit code, and captured output.
pub(crate) fn run_ci_gate(resolved: &crate::config::Resolved) -> Result<CiGateResult> {
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
        return Ok(CiGateResult {
            success: true,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    if command.is_empty() {
        bail!(
            "CI gate command is empty but CI gate is enabled. Set agent.ci_gate_command or disable the gate with agent.ci_gate_enabled=false."
        );
    }

    logging::with_scope(&format!("CI gate ({command})"), || {
        let output = runutil::shell_command(command)
            .current_dir(&resolved.repo_root)
            .env_remove(crate::config::QUEUE_PATH_OVERRIDE_ENV)
            .env_remove(crate::config::DONE_PATH_OVERRIDE_ENV)
            .env_remove(crate::config::REPO_ROOT_OVERRIDE_ENV)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| {
                format!(
                    "run CI gate command '{}' in {}",
                    command,
                    resolved.repo_root.display()
                )
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let success = output.status.success();
        let exit_code = output.status.code();

        // Log output on failure for debugging
        if !success {
            log::error!(
                "CI gate ({command}) failed with exit code {:?}. stdout: {}",
                exit_code,
                truncate_for_log(&stdout, 2000)
            );
            log::error!(
                "CI gate ({command}) stderr: {}",
                truncate_for_log(&stderr, 2000)
            );
        }

        Ok(CiGateResult {
            success,
            exit_code,
            stdout,
            stderr,
        })
    })
}

fn strict_ci_gate_compliance_message(
    resolved: &crate::config::Resolved,
    result: &CiGateResult,
) -> String {
    let cmd = ci_gate_command_label(resolved);

    // Include last N lines of output in the message
    let output_snippet = format_ci_output_for_message(&result.stdout, &result.stderr, 100);

    // Format exit code as a number, using -1 if unavailable (e.g., killed by signal)
    let exit_code_display = result.exit_code.unwrap_or(-1);

    format!(
        r#"CI gate ({cmd}): CI failed with exit code {exit_code_display}.

{output_snippet}

Run '{cmd}' again WITHOUT tail/head truncation to see the full output. Fix the errors above before continuing. You MUST see the CI gate pass before this turn can end.

COMMON PATTERNS:
- "ruff failed: TOML parse error" -> Check pyproject.toml for invalid values at the mentioned line
- "unknown variant X, expected one of Y" -> X is invalid, use one of Y instead
- "format-check failed" -> Run the formatter to see what needs changing
- "lint-check failed" -> Run the linter directly to see errors

NO skipping tests, half-assed patches, or sloppy shortcuts."#
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
        let result = run_ci_gate(resolved)?;

        if result.success {
            break;
        }

        if continue_session.ci_failure_retry_count < CI_GATE_AUTO_RETRY_LIMIT {
            continue_session.ci_failure_retry_count =
                continue_session.ci_failure_retry_count.saturating_add(1);
            let attempt = continue_session.ci_failure_retry_count;

            log::warn!(
                "CI gate failed; auto-sending strict compliance Continue message to agent (attempt {}/{})",
                attempt,
                CI_GATE_AUTO_RETRY_LIMIT
            );

            // Include the CI output in the compliance message
            let message = strict_ci_gate_compliance_message(resolved, &result);
            let (output, elapsed) =
                super::resume_continue_session(resolved, continue_session, &message, plugins)?;
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
                let (output, elapsed) =
                    super::resume_continue_session(resolved, continue_session, &message, plugins)?;
                on_resume(&output, elapsed)?;
                continue;
            }
            _ => {
                let exit_code_display = result.exit_code.unwrap_or(-1);
                bail!(
                    "{} Error: CI failed with exit code {exit_code_display}",
                    runutil::format_revert_failure_message(
                        "CI gate failed after changes. Fix issues reported by CI and rerun.",
                        outcome,
                    ),
                );
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
        // Should succeed without running anything, returning success
        let result = run_ci_gate(&resolved)?;
        assert!(result.success);
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

        // Verify the result is successful (env vars were stripped)
        let ci_result = result?;
        assert!(ci_result.success);
        Ok(())
    }

    #[test]
    fn run_ci_gate_captures_output() -> Result<()> {
        let temp = TempDir::new()?;
        let command = if cfg!(windows) {
            "powershell -NoProfile -Command \"Write-Output 'stdout text'; Write-Error 'stderr text'; exit 1\""
        } else {
            "sh -c 'echo stdout text; echo stderr text >&2; exit 1'"
        };
        let resolved = resolved_with_ci_command(temp.path(), Some(command.to_string()), true);
        let result = run_ci_gate(&resolved)?;

        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
        assert!(result.stdout.contains("stdout text"));
        assert!(result.stderr.contains("stderr text"));
        Ok(())
    }

    #[test]
    fn format_ci_output_includes_stderr_first() {
        let stdout = "line1\nline2\nline3";
        let stderr = "error1\nerror2";
        let result = format_ci_output_for_message(stdout, stderr, 10);

        // stderr should appear in output
        assert!(result.contains("error1"));
        assert!(result.contains("error2"));
    }

    #[test]
    fn format_ci_output_truncates_to_max_lines() {
        let stdout = (1..=200)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let stderr = "";
        let result = format_ci_output_for_message(&stdout, stderr, 50);

        // Should include "Last 50 lines"
        assert!(result.contains("Last 50 lines"));
        // Should include line 151 (line 200 - 50 + 1)
        assert!(result.contains("line151"));
        // Should NOT include line 150
        assert!(!result.contains("line150"));
    }

    #[test]
    fn format_ci_output_handles_empty() {
        let result = format_ci_output_for_message("", "", 100);
        assert!(result.contains("No output captured"));
    }

    #[test]
    fn compliance_message_includes_exit_code_and_output() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), None, true);
        let result = CiGateResult {
            success: false,
            exit_code: Some(2),
            stdout: "test output".to_string(),
            stderr: "error: ruff failed".to_string(),
        };

        let msg = strict_ci_gate_compliance_message(&resolved, &result);
        // Should show numeric exit code, not Debug format like "Some(2)"
        assert!(
            msg.contains("exit code 2"),
            "Expected 'exit code 2', got: {msg}"
        );
        assert!(msg.contains("ruff failed"));
    }

    #[test]
    fn compliance_message_includes_troubleshooting_patterns() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), None, true);
        let result = CiGateResult {
            success: false,
            exit_code: Some(2),
            stdout: String::new(),
            stderr: String::new(),
        };

        let msg = strict_ci_gate_compliance_message(&resolved, &result);
        assert!(msg.contains("TOML parse error"));
        assert!(msg.contains("unknown variant"));
        assert!(msg.contains("format-check failed"));
        assert!(msg.contains("lint-check failed"));
    }

    #[test]
    fn truncate_for_log_shows_end_of_string() {
        let long = "a".repeat(3000);
        let truncated = truncate_for_log(&long, 100);
        assert!(truncated.starts_with("..."));
        // Should have exactly 103 characters: "..." + 100 'a's
        assert_eq!(truncated.len(), 103);
    }

    #[test]
    fn truncate_for_log_returns_full_if_short() {
        let short = "hello world";
        let truncated = truncate_for_log(short, 100);
        assert_eq!(truncated, short);
    }

    #[test]
    fn truncate_for_log_handles_multibyte_utf8() {
        // Test with multi-byte UTF-8 characters (emoji = 4 bytes each)
        let long = "😀".repeat(100); // 100 emoji = 400 bytes
        let truncated = truncate_for_log(&long, 10); // Keep last 10 chars

        // Should not panic and should produce valid UTF-8
        assert!(truncated.starts_with("..."));
        // After "...", should have exactly 10 emoji characters
        let emoji_part = &truncated[3..]; // Skip "..."
        assert_eq!(emoji_part.chars().count(), 10);
    }

    #[test]
    fn truncate_for_log_handles_empty_string() {
        let truncated = truncate_for_log("", 100);
        assert_eq!(truncated, "");
    }
}
