//! CI gate execution for post-run supervision.
//!
//! Responsibilities:
//! - Execute the configured CI gate command (default: make ci).
//! - Capture stdout/stderr for compliance messages.
//! - Detect common error patterns and provide specific guidance.
//! - Provide command label for error messages.
//!
//! Not handled here:
//! - Queue maintenance (see queue_ops.rs).
//! - Git operations (see git_ops.rs).
//!
//! Invariants/assumptions:
//! - CI gate command is configured or defaults to "make ci".
//! - Command output is captured (not inherited) to include in compliance messages.
//! - Error pattern detection is best-effort; undetected patterns fall back to generic guidance.

use super::logging;
use crate::constants::limits::{CI_FAILURE_ESCALATION_THRESHOLD, CI_GATE_AUTO_RETRY_LIMIT};
use crate::runutil;
use anyhow::{Context, Result, bail};
use std::process::Stdio;

// ============================================================================
// CI Error Pattern Detection
// ============================================================================

/// Detected error pattern from CI output with actionable guidance.
#[derive(Debug, Clone)]
pub(crate) struct DetectedErrorPattern {
    /// Human-readable pattern name (e.g., "TOML parse error")
    pub pattern_type: &'static str,
    /// File path mentioned in the error, if extractable
    pub file_path: Option<String>,
    /// Line number mentioned in the error, if extractable
    pub line_number: Option<u32>,
    /// Invalid value that caused the error
    pub invalid_value: Option<String>,
    /// Valid values/alternatives mentioned in the error
    pub valid_values: Option<String>,
    /// Specific actionable guidance for this pattern
    pub guidance: &'static str,
}

// Guidance templates for common error patterns
const TOML_PARSE_ERROR_GUIDANCE: &str =
    "Read the TOML file at the mentioned line and fix the syntax error or invalid value.";
const UNKNOWN_VARIANT_GUIDANCE: &str =
    "Replace the invalid value with one of the valid options listed in the error message.";
const RUFF_PYPROJECT_GUIDANCE: &str = "Check pyproject.toml for invalid ruff configuration. Common issues: invalid target-version, unknown lint rules.";
const FORMAT_CHECK_GUIDANCE: &str = "Run the formatter directly to see what needs changing.";
const LINT_CHECK_GUIDANCE: &str = "Run the linter directly to see the specific errors.";

/// Find the first byte index of `needle` in `haystack` using ASCII case-insensitive matching.
///
/// Returns an index into the original `haystack` so callers can safely slice without
/// mixing indices from transformed strings (e.g., from `to_lowercase()`).
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if haystack.len() < needle.len() {
        return None;
    }

    for (idx, _) in haystack.char_indices() {
        if let Some(candidate) = haystack.get(idx..idx + needle.len())
            && candidate.eq_ignore_ascii_case(needle)
        {
            return Some(idx);
        }
    }

    None
}

/// Extract line number from error output.
///
/// Looks for patterns like:
/// - "at line N"
/// - "line N, column M"
/// - ":N:M" suffix on paths (e.g., "file.rs:44:18")
fn extract_line_number(output: &str) -> Option<u32> {
    let lower = output.to_lowercase();

    // Pattern: "at line N" or "line N, column M"
    if let Some(pos) = lower.find("line ") {
        let after = &lower[pos + 5..];
        // Get the first token after "line "
        if let Some(token) = after.split_whitespace().next() {
            // Trim trailing punctuation like colons or commas
            let cleaned = token.trim_end_matches(':').trim_end_matches(',');
            if let Ok(num) = cleaned.parse::<u32>() {
                return Some(num);
            }
        }
    }

    // Pattern: ":N:M" suffix (e.g., "file.rs:44:18")
    // Look for colon followed by digits at the end of a word
    for part in lower.split_whitespace() {
        // Check for pattern like "pyproject.toml:44:18"
        // We need to find the first colon after the filename
        if let Some(first_colon) = part.find(':') {
            let after_first = &part[first_colon + 1..];
            // The next segment should be the line number
            if let Some(line_str) = after_first.split(':').next()
                && let Ok(num) = line_str.parse::<u32>()
                && num > 0
                && num < 100000
            {
                // Sanity check for line numbers
                return Some(num);
            }
        }
    }

    None
}

/// Extract invalid value from unknown variant errors.
///
/// Pattern: "unknown variant `VALUE`"
fn extract_invalid_value(output: &str) -> Option<String> {
    if let Some(pos) = find_ascii_case_insensitive(output, "unknown variant") {
        let after = &output[pos..];
        // Look for backtick-delimited value
        if let Some(start) = after.find('`') {
            let rest = &after[start + 1..];
            if let Some(end) = rest.find('`') {
                return Some(rest[..end].to_string());
            }
        }
    }

    None
}

/// Extract valid alternatives from unknown variant errors.
///
/// Pattern: "expected one of A, B, C"
fn extract_valid_values(output: &str) -> Option<String> {
    const PREFIX: &str = "expected one of";
    if let Some(pos) = find_ascii_case_insensitive(output, PREFIX) {
        let after = &output[pos + PREFIX.len()..];
        // Take everything up to common terminators (but NOT comma, since values are comma-separated)
        let end_pos = after
            .find('\n')
            .or_else(|| after.find('.'))
            .unwrap_or(after.len());
        let values = after[..end_pos].trim();
        if !values.is_empty() {
            return Some(values.to_string());
        }
    }

    None
}

/// Infer file path from error context.
///
/// For errors that don't explicitly mention a file, infer based on error type.
fn infer_file_path(output: &str) -> Option<String> {
    let lower = output.to_lowercase();

    // Check for explicitly mentioned files
    for filename in &["pyproject.toml", "cargo.toml", "rustfmt.toml", ".toml"] {
        if lower.contains(filename) {
            // Try to extract the specific filename
            for word in lower.split_whitespace() {
                if word.contains(".toml") || word.ends_with(".toml") {
                    // Clean up any trailing punctuation
                    let cleaned = word.trim_end_matches(':').trim_end_matches(',');
                    return Some(cleaned.to_string());
                }
            }
            return Some(filename.to_string());
        }
    }

    // Infer from ruff context
    if lower.contains("ruff") && lower.contains("parse") {
        return Some("pyproject.toml".to_string());
    }

    None
}

/// Detect TOML parse errors with file/line information.
///
/// Pattern: "TOML parse error at line N, column M" or "parse error at line N"
fn detect_toml_parse_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();

    if !lower.contains("toml") || !lower.contains("parse") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "TOML parse error",
        file_path: infer_file_path(output),
        line_number: extract_line_number(output),
        invalid_value: extract_invalid_value(output),
        valid_values: extract_valid_values(output),
        guidance: TOML_PARSE_ERROR_GUIDANCE,
    })
}

/// Detect "unknown variant" enum errors.
///
/// Pattern: "unknown variant `X`, expected one of A, B, C"
fn detect_unknown_variant_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();

    if !lower.contains("unknown variant") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "Unknown variant error",
        file_path: infer_file_path(output),
        line_number: extract_line_number(output),
        invalid_value: extract_invalid_value(output),
        valid_values: extract_valid_values(output),
        guidance: UNKNOWN_VARIANT_GUIDANCE,
    })
}

/// Detect ruff-specific errors.
///
/// Pattern: "ruff failed:" or tool-specific prefixes
fn detect_ruff_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();

    if !lower.contains("ruff") {
        return None;
    }

    // If it's also a TOML parse error, let that handler take precedence
    if lower.contains("toml") && lower.contains("parse") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "Ruff error",
        file_path: Some("pyproject.toml".to_string()),
        line_number: extract_line_number(output),
        invalid_value: extract_invalid_value(output),
        valid_values: extract_valid_values(output),
        guidance: RUFF_PYPROJECT_GUIDANCE,
    })
}

/// Detect format-check failures.
///
/// Pattern: "format-check failed" or "format check failed"
fn detect_format_check_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();

    if !lower.contains("format") || !lower.contains("failed") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "Format check failure",
        file_path: None,
        line_number: None,
        invalid_value: None,
        valid_values: None,
        guidance: FORMAT_CHECK_GUIDANCE,
    })
}

/// Detect lint-check failures.
///
/// Pattern: "lint-check failed" or "lint check failed"
fn detect_lint_check_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();

    if !lower.contains("lint") || !lower.contains("failed") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "Lint check failure",
        file_path: None,
        line_number: None,
        invalid_value: None,
        valid_values: None,
        guidance: LINT_CHECK_GUIDANCE,
    })
}

/// Main entry point to detect CI error patterns.
///
/// Scans combined stdout/stderr for known error patterns and returns
/// the most specific/relevant pattern found.
fn detect_ci_error_pattern(stdout: &str, stderr: &str) -> Option<DetectedErrorPattern> {
    let combined = format!("{}\n{}", stderr, stdout);

    // Try patterns in order of specificity
    detect_toml_parse_error(&combined)
        .or_else(|| detect_unknown_variant_error(&combined))
        .or_else(|| detect_ruff_error(&combined))
        .or_else(|| detect_format_check_error(&combined))
        .or_else(|| detect_lint_check_error(&combined))
}

/// Get a stable key representing the error pattern for comparison.
/// Returns None if no pattern detected, or Some(pattern_type) if detected.
#[cfg(test)]
fn get_error_pattern_key(result: &CiGateResult) -> Option<String> {
    detect_ci_error_pattern(&result.stdout, &result.stderr).map(|p| p.pattern_type.to_string())
}

/// Format detected pattern into actionable guidance for compliance message.
fn format_detected_pattern(pattern: &DetectedErrorPattern) -> String {
    let mut guidance = format!("\n## DETECTED ERROR: {}\n", pattern.pattern_type);

    if let Some(file) = &pattern.file_path {
        guidance.push_str(&format!("- **File**: `{}`\n", file));
    }
    if let Some(line) = pattern.line_number {
        guidance.push_str(&format!("- **Line**: {}\n", line));
    }
    if let Some(invalid) = &pattern.invalid_value {
        guidance.push_str(&format!("- **Invalid value**: `{}`\n", invalid));
    }
    if let Some(valid) = &pattern.valid_values {
        guidance.push_str(&format!("- **Valid options**: {}\n", valid));
    }

    guidance.push_str(&format!("\n**Action**: {}\n", pattern.guidance));
    guidance
}

/// Result of running the CI gate command.
#[derive(Debug)]
#[allow(dead_code)] // success field used in tests only
pub(crate) struct CiGateResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// CI gate failure with captured output for logging.
///
/// This error type is used when CI fails (non-zero exit code).
/// The `Display` impl includes truncated stdout/stderr and detected
/// error patterns, allowing `with_scope` to log a rich error message.
#[derive(Debug)]
pub(crate) struct CiFailure {
    /// Exit code from the CI command
    pub exit_code: Option<i32>,
    /// Full stdout (kept for compliance messages)
    pub stdout: String,
    /// Full stderr (kept for compliance messages)
    pub stderr: String,
    /// Detected error pattern type, if any
    pub error_pattern: Option<&'static str>,
}

impl std::fmt::Display for CiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let exit_code = self.exit_code.unwrap_or(-1);
        write!(f, "CI failed with exit code {}", exit_code)?;

        if let Some(pattern) = &self.error_pattern {
            write!(f, " [{}]", pattern)?;
        }

        // Include truncated output for immediate visibility in logs
        let stderr_preview = truncate_for_log(&self.stderr, 500);
        let stdout_preview = truncate_for_log(&self.stdout, 500);

        if !stderr_preview.is_empty() {
            write!(f, "\n>>> stderr:\n{}", stderr_preview)?;
        }
        if !stdout_preview.is_empty() {
            write!(f, "\n>>> stdout:\n{}", stdout_preview)?;
        }

        Ok(())
    }
}

impl std::error::Error for CiFailure {}

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
/// Shows BOTH the start (early errors like format/lint) and end (test failures)
/// of CI output to ensure the agent sees all relevant errors.
///
/// stderr is included first since errors typically appear there.
fn format_ci_output_for_message(
    stdout: &str,
    stderr: &str,
    max_head_lines: usize,
    max_tail_lines: usize,
) -> String {
    let mut lines: Vec<&str> = Vec::new();

    // Include stderr first (usually contains errors)
    lines.extend(stderr.lines());
    lines.extend(stdout.lines());

    let total_lines = lines.len();

    if total_lines == 0 {
        return "No output captured.".to_string();
    }

    // If output fits within budget, show everything
    if total_lines <= max_head_lines + max_tail_lines {
        return format!(
            "CI output ({} lines):\n```\n{}\n```",
            total_lines,
            lines.join("\n")
        );
    }

    // Show head (early errors) and tail (test failures)
    let head: Vec<&str> = lines.iter().take(max_head_lines).copied().collect();
    let tail_start = total_lines.saturating_sub(max_tail_lines);
    let tail: Vec<&str> = lines.iter().skip(tail_start).copied().collect();
    let omitted = total_lines - max_head_lines - max_tail_lines;

    format!(
        "CI output ({} lines total, showing first {} and last {}):\n\
         ```
         {}
         ```

         ... {} lines omitted ...

         ```
         {}
         ```",
        total_lines,
        max_head_lines,
        max_tail_lines,
        head.join("\n"),
        omitted,
        tail.join("\n")
    )
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
        let mut cmd = runutil::shell_command(command);
        cmd.current_dir(&resolved.repo_root);
        runutil::sanitize_run_scoped_overrides(&mut cmd);
        let output = cmd
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

        if success {
            Ok(CiGateResult {
                success,
                exit_code,
                stdout,
                stderr,
            })
        } else {
            // Detect error pattern for logging context
            let detected = detect_ci_error_pattern(&stdout, &stderr);
            let error_pattern = detected.as_ref().map(|p| p.pattern_type);

            // Return CiFailure so with_scope logs it with ERROR level
            // The Display impl includes truncated output for immediate visibility
            Err(CiFailure {
                exit_code,
                stdout,
                stderr,
                error_pattern,
            }
            .into())
        }
    })
}

fn strict_ci_gate_compliance_message(
    resolved: &crate::config::Resolved,
    result: &CiGateResult,
) -> String {
    let cmd = ci_gate_command_label(resolved);

    // Include head (early errors) and tail (test failures) of output in the message
    let output_snippet = format_ci_output_for_message(&result.stdout, &result.stderr, 50, 50);

    // Format exit code as a number, using -1 if unavailable (e.g., killed by signal)
    let exit_code_display = result.exit_code.unwrap_or(-1);

    // Detect error patterns and generate specific guidance
    let detected = detect_ci_error_pattern(&result.stdout, &result.stderr);
    let specific_guidance = detected
        .as_ref()
        .map(format_detected_pattern)
        .unwrap_or_default();

    format!(
        r#"CI gate ({cmd}): CI failed with exit code {exit_code_display}.

{output_snippet}
{specific_guidance}Fix the errors above before continuing. You MUST see the CI gate pass before this turn can end.

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
        // run_ci_gate returns Ok(CiGateResult) on success, Err(CiFailure) on CI failure
        let result = match run_ci_gate(resolved) {
            Ok(_) => {
                // CI passed - reset error tracking and exit loop
                continue_session.last_ci_error_pattern = None;
                continue_session.consecutive_same_error_count = 0;
                return Ok(());
            }
            Err(err) => {
                // Check if this is a CI failure (retryable) or another error
                err.downcast::<CiFailure>()?
            }
        };

        // Get current error pattern and update consecutive count
        let current_pattern = result.error_pattern.as_ref().map(|p| p.to_string());

        match (&continue_session.last_ci_error_pattern, &current_pattern) {
            (Some(last), Some(current)) if last == current => {
                continue_session.consecutive_same_error_count = continue_session
                    .consecutive_same_error_count
                    .saturating_add(1);
            }
            _ => {
                // Different error or no pattern - reset counter
                continue_session.consecutive_same_error_count = 1;
            }
        }
        continue_session.last_ci_error_pattern = current_pattern.clone();

        // Check for escalation threshold (same error repeated N times)
        if continue_session.consecutive_same_error_count >= CI_FAILURE_ESCALATION_THRESHOLD {
            log::error!(
                "CI gate failed {} times with same error pattern '{}'; escalating",
                continue_session.consecutive_same_error_count,
                current_pattern.as_deref().unwrap_or("unknown")
            );

            let detected = detect_ci_error_pattern(&result.stdout, &result.stderr);
            let specific_guidance = detected
                .as_ref()
                .map(format_detected_pattern)
                .unwrap_or_default();

            let outcome = runutil::apply_git_revert_mode(
                &resolved.repo_root,
                git_revert_mode,
                "CI failure escalation",
                revert_prompt,
            )?;

            bail!(
                "{} Error: CI failed {} consecutive times with the same error.\n\n\
                 The agent is not making progress on this issue.\n\n\
                 Error pattern: {}\n\n\
                 {}\n\n\
                 MANUAL INTERVENTION REQUIRED: The automated compliance messages \
                 are not resolving this CI failure. Please investigate the root cause \
                 directly and fix it before re-running.",
                runutil::format_revert_failure_message(
                    "CI gate repeated failure escalation.",
                    outcome,
                ),
                continue_session.consecutive_same_error_count,
                current_pattern.as_deref().unwrap_or("unrecognized"),
                specific_guidance
            );
        }

        // Existing retry logic for attempts below threshold
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
            // Build CiGateResult from CiFailure for message formatting
            let gate_result = CiGateResult {
                success: false,
                exit_code: result.exit_code,
                stdout: result.stdout.clone(),
                stderr: result.stderr.clone(),
            };
            let message = strict_ci_gate_compliance_message(resolved, &gate_result);
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
        let err = run_ci_gate(&resolved).unwrap_err();

        // CI failure now returns Err(CiFailure)
        let ci_failure = err.downcast::<CiFailure>().unwrap();
        assert_eq!(ci_failure.exit_code, Some(1));
        assert!(ci_failure.stdout.contains("stdout text"));
        assert!(ci_failure.stderr.contains("stderr text"));
        Ok(())
    }

    #[test]
    fn format_ci_output_includes_stderr_first() {
        let stdout = "line1\nline2\nline3";
        let stderr = "error1\nerror2";
        let result = format_ci_output_for_message(stdout, stderr, 50, 50);

        // stderr should appear in output
        assert!(result.contains("error1"));
        assert!(result.contains("error2"));
    }

    #[test]
    fn format_ci_output_shows_head_and_tail() {
        let stdout = (1..=200)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let stderr = "";

        // Request 50 head + 50 tail
        let result = format_ci_output_for_message(&stdout, stderr, 50, 50);

        // Should show total line count
        assert!(result.contains("200 lines total"));

        // Should include early lines (format/lint errors appear here)
        assert!(result.contains("line1"));
        assert!(result.contains("line50"));

        // Should include late lines (test failures appear here)
        assert!(result.contains("line151"));
        assert!(result.contains("line200"));

        // Should NOT include middle lines
        assert!(!result.contains("line51"));
        assert!(!result.contains("line100"));
        assert!(!result.contains("line150"));

        // Should indicate truncation
        assert!(result.contains("100 lines omitted"));
    }

    #[test]
    fn format_ci_output_shows_all_when_small() {
        let stdout = "line1\nline2\nline3";
        let stderr = "";

        let result = format_ci_output_for_message(stdout, stderr, 50, 50);

        // Should show all without truncation
        assert!(result.contains("3 lines)"));
        assert!(result.contains("line1"));
        assert!(result.contains("line3"));
        assert!(!result.contains("omitted"));
    }

    #[test]
    fn format_ci_output_handles_empty() {
        let result = format_ci_output_for_message("", "", 50, 50);
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
    fn compliance_message_contains_required_enforcement_language() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), None, true);
        let result = CiGateResult {
            success: false,
            exit_code: Some(2),
            stdout: "fmt-check passed".to_string(),
            stderr: "ruff failed: TOML parse error".to_string(),
        };

        let msg = strict_ci_gate_compliance_message(&resolved, &result);
        assert!(
            msg.contains("CI gate (make ci): CI failed with exit code 2"),
            "Expected CI gate prefix with exit code, got: {msg}"
        );
        assert!(
            msg.contains("Fix the errors above before continuing."),
            "Expected remediation instruction, got: {msg}"
        );
        assert!(
            msg.contains("COMMON PATTERNS:"),
            "Expected COMMON PATTERNS section, got: {msg}"
        );
        assert!(
            msg.contains("ruff failed: TOML parse error"),
            "Expected CI output context in message, got: {msg}"
        );
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

    // ========================================================================
    // CiFailure Tests
    // ========================================================================

    #[test]
    fn ci_failure_display_includes_exit_code() {
        let failure = CiFailure {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: String::new(),
            error_pattern: None,
        };

        let msg = failure.to_string();
        assert!(
            msg.contains("exit code 1"),
            "Expected 'exit code 1', got: {msg}"
        );
    }

    #[test]
    fn ci_failure_display_includes_error_pattern() {
        let failure = CiFailure {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: String::new(),
            error_pattern: Some("TOML parse error"),
        };

        let msg = failure.to_string();
        assert!(
            msg.contains("[TOML parse error]"),
            "Expected pattern, got: {msg}"
        );
    }

    #[test]
    fn ci_failure_display_includes_truncated_output() {
        let failure = CiFailure {
            exit_code: Some(1),
            stdout: "test output".to_string(),
            stderr: "TOML parse error at line 44".to_string(),
            error_pattern: Some("TOML parse error"),
        };

        let msg = failure.to_string();
        assert!(
            msg.contains(">>> stderr:"),
            "Expected stderr section, got: {msg}"
        );
        assert!(
            msg.contains(">>> stdout:"),
            "Expected stdout section, got: {msg}"
        );
        assert!(
            msg.contains("TOML parse error at line 44"),
            "Expected error message, got: {msg}"
        );
    }

    #[test]
    fn ci_failure_truncates_long_output() {
        let long_output = "x".repeat(1000);
        let failure = CiFailure {
            exit_code: Some(1),
            stdout: long_output.clone(),
            stderr: String::new(),
            error_pattern: None,
        };

        let msg = failure.to_string();
        // Should be truncated, not full 1000 chars
        assert!(
            msg.len() < 800,
            "Message should be truncated, got length {}",
            msg.len()
        );
        assert!(
            msg.contains("..."),
            "Expected truncation marker, got: {msg}"
        );
    }

    #[test]
    fn ci_failure_handles_missing_exit_code() {
        let failure = CiFailure {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error_pattern: None,
        };

        let msg = failure.to_string();
        assert!(
            msg.contains("exit code -1"),
            "Expected -1 for missing exit code, got: {msg}"
        );
    }

    // ========================================================================
    // Pattern Detection Tests
    // ========================================================================

    #[test]
    fn detect_toml_parse_error_extracts_line_number() {
        let output = "ruff failed: TOML parse error at line 44, column 18: unknown variant `py314`";
        let pattern = detect_toml_parse_error(output).unwrap();
        assert_eq!(pattern.line_number, Some(44));
        assert_eq!(pattern.pattern_type, "TOML parse error");
    }

    #[test]
    fn detect_toml_parse_error_returns_none_for_non_toml() {
        let output = "Some random error message";
        assert!(detect_toml_parse_error(output).is_none());
    }

    #[test]
    fn detect_unknown_variant_extracts_values() {
        let output =
            "unknown variant `py314`, expected one of py37, py38, py39, py310, py311, py312, py313";
        let pattern = detect_unknown_variant_error(output).unwrap();
        assert_eq!(pattern.invalid_value, Some("py314".to_string()));
        assert!(pattern.valid_values.unwrap().contains("py313"));
        assert_eq!(pattern.pattern_type, "Unknown variant error");
    }

    #[test]
    fn detect_unknown_variant_returns_none_for_non_variant() {
        let output = "Some error without variant";
        assert!(detect_unknown_variant_error(output).is_none());
    }

    #[test]
    fn detect_ruff_error_returns_pattern() {
        let output = "ruff failed with some error";
        let pattern = detect_ruff_error(output).unwrap();
        assert_eq!(pattern.pattern_type, "Ruff error");
        assert_eq!(pattern.file_path, Some("pyproject.toml".to_string()));
    }

    #[test]
    fn detect_format_check_error_returns_pattern() {
        let output = "format-check failed";
        let pattern = detect_format_check_error(output).unwrap();
        assert_eq!(pattern.pattern_type, "Format check failure");
    }

    #[test]
    fn detect_lint_check_error_returns_pattern() {
        let output = "lint check failed";
        let pattern = detect_lint_check_error(output).unwrap();
        assert_eq!(pattern.pattern_type, "Lint check failure");
    }

    #[test]
    fn detect_ci_error_pattern_combines_stdout_stderr() {
        let stdout = "Some output";
        let stderr = "TOML parse error at line 10";
        let pattern = detect_ci_error_pattern(stdout, stderr).unwrap();
        assert_eq!(pattern.line_number, Some(10));
    }

    #[test]
    fn detect_ci_error_pattern_returns_none_on_clean_output() {
        let output = "All tests passed!";
        assert!(detect_ci_error_pattern(output, "").is_none());
    }

    #[test]
    fn extract_line_number_from_at_line_pattern() {
        let output = "Error at line 42";
        assert_eq!(extract_line_number(output), Some(42));
    }

    #[test]
    fn extract_line_number_from_colon_pattern() {
        let output = "pyproject.toml:44:18: error";
        assert_eq!(extract_line_number(output), Some(44));
    }

    #[test]
    fn extract_line_number_returns_none_when_not_present() {
        let output = "No line number here";
        assert!(extract_line_number(output).is_none());
    }

    #[test]
    fn extract_invalid_value_finds_backtick_value() {
        let output = "unknown variant `py314`, expected...";
        assert_eq!(extract_invalid_value(output), Some("py314".to_string()));
    }

    #[test]
    fn extract_invalid_value_handles_unicode_prefix() {
        let output = "İstanbul: unknown variant `py314`, expected one of py37, py313";
        assert_eq!(extract_invalid_value(output), Some("py314".to_string()));
    }

    #[test]
    fn extract_valid_values_finds_expected_list() {
        let output = "expected one of py37, py38, py313";
        assert_eq!(
            extract_valid_values(output),
            Some("py37, py38, py313".to_string())
        );
    }

    #[test]
    fn extract_valid_values_handles_unicode_prefix() {
        let output = "İstanbul: expected one of py37, py313.";
        assert_eq!(
            extract_valid_values(output),
            Some("py37, py313".to_string())
        );
    }

    #[test]
    fn infer_file_path_extracts_explicit_toml_file() {
        let output = "ruff failed parsing pyproject.toml, unknown variant `py314`";
        assert_eq!(infer_file_path(output), Some("pyproject.toml".to_string()));
    }

    #[test]
    fn infer_file_path_infers_pyproject_from_ruff_parse_context() {
        let output = "ruff failed: parse error at line 5";
        assert_eq!(infer_file_path(output), Some("pyproject.toml".to_string()));
    }

    #[test]
    fn compliance_message_includes_detected_toml_error() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), None, true);
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr:
                "TOML parse error at line 44: unknown variant `py314`, expected one of py37, py313"
                    .to_string(),
        };

        let msg = strict_ci_gate_compliance_message(&resolved, &result);
        assert!(
            msg.contains("DETECTED ERROR"),
            "Should contain DETECTED ERROR section"
        );
        assert!(
            msg.contains("TOML parse error"),
            "Should identify error type"
        );
        assert!(msg.contains("**Line**"), "Should show Line label");
        assert!(msg.contains("44"), "Should show line 44");
        assert!(msg.contains("py314"), "Should show invalid value");
    }

    #[test]
    fn compliance_message_includes_detected_unknown_variant() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), None, true);
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "unknown variant `foo`, expected one of bar, baz".to_string(),
        };

        let msg = strict_ci_gate_compliance_message(&resolved, &result);
        assert!(
            msg.contains("DETECTED ERROR"),
            "Should contain DETECTED ERROR section"
        );
        assert!(
            msg.contains("Unknown variant error"),
            "Should identify error type"
        );
        assert!(msg.contains("`foo`"), "Should show invalid value");
        assert!(msg.contains("bar, baz"), "Should show valid options");
    }

    #[test]
    fn compliance_message_no_detected_section_on_clean_output() {
        let temp = TempDir::new().unwrap();
        let resolved = resolved_with_ci_command(temp.path(), None, true);
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: "build failed".to_string(),
            stderr: String::new(),
        };

        let msg = strict_ci_gate_compliance_message(&resolved, &result);
        assert!(
            !msg.contains("DETECTED ERROR"),
            "Should NOT contain DETECTED ERROR section for unrecognized errors"
        );
        // Should still have common patterns
        assert!(msg.contains("COMMON PATTERNS"));
    }

    #[test]
    fn format_detected_pattern_includes_all_fields() {
        let pattern = DetectedErrorPattern {
            pattern_type: "Test error",
            file_path: Some("test.toml".to_string()),
            line_number: Some(10),
            invalid_value: Some("bad_value".to_string()),
            valid_values: Some("good1, good2".to_string()),
            guidance: "Fix the error",
        };

        let formatted = format_detected_pattern(&pattern);
        assert!(formatted.contains("Test error"));
        assert!(formatted.contains("test.toml"));
        assert!(formatted.contains("10"));
        assert!(formatted.contains("bad_value"));
        assert!(formatted.contains("good1, good2"));
        assert!(formatted.contains("Fix the error"));
    }

    // ========================================================================
    // Error Pattern Key Tests
    // ========================================================================

    #[test]
    fn get_error_pattern_key_returns_pattern_type() {
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "TOML parse error at line 44".to_string(),
        };
        assert_eq!(
            get_error_pattern_key(&result),
            Some("TOML parse error".to_string())
        );
    }

    #[test]
    fn get_error_pattern_key_returns_none_for_unrecognized() {
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: "some random output".to_string(),
            stderr: String::new(),
        };
        assert_eq!(get_error_pattern_key(&result), None);
    }

    #[test]
    fn get_error_pattern_key_detects_unknown_variant() {
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "unknown variant `py314`, expected one of py37, py38".to_string(),
        };
        assert_eq!(
            get_error_pattern_key(&result),
            Some("Unknown variant error".to_string())
        );
    }

    #[test]
    fn get_error_pattern_key_detects_format_check() {
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "format-check failed".to_string(),
        };
        assert_eq!(
            get_error_pattern_key(&result),
            Some("Format check failure".to_string())
        );
    }

    #[test]
    fn get_error_pattern_key_detects_lint_check() {
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "lint check failed".to_string(),
        };
        assert_eq!(
            get_error_pattern_key(&result),
            Some("Lint check failure".to_string())
        );
    }

    #[test]
    fn get_error_pattern_key_combines_stdout_stderr() {
        let result = CiGateResult {
            success: false,
            exit_code: Some(1),
            stdout: "some output".to_string(),
            stderr: "TOML parse error at line 10".to_string(),
        };
        assert_eq!(
            get_error_pattern_key(&result),
            Some("TOML parse error".to_string())
        );
    }

    // ========================================================================
    // Table-Driven Pattern Detection Tests
    // ========================================================================

    #[test]
    fn detect_ci_error_pattern_cases() {
        struct Case {
            stdout: &'static str,
            stderr: &'static str,
            want: Option<&'static str>,
            want_line: Option<u32>,
            want_invalid: Option<&'static str>,
        }

        let cases = [
            Case {
                stdout: "",
                stderr: "ruff failed: TOML parse error at line 44, column 18",
                want: Some("TOML parse error"),
                want_line: Some(44),
                want_invalid: None,
            },
            Case {
                stdout: "",
                stderr: "unknown variant `py314`, expected one of py37, py313",
                want: Some("Unknown variant error"),
                want_line: None,
                want_invalid: Some("py314"),
            },
            Case {
                stdout: "",
                stderr: "TOML parse error at line 10: unknown variant `foo`",
                want: Some("TOML parse error"),
                want_line: Some(10),
                want_invalid: Some("foo"),
            },
            Case {
                stdout: "TOML parse error",
                stderr: "",
                want: Some("TOML parse error"),
                want_line: None,
                want_invalid: None,
            },
            Case {
                stdout: "",
                stderr: "ruff: error checking configuration",
                want: Some("Ruff error"),
                want_line: None,
                want_invalid: None,
            },
            Case {
                stdout: "",
                stderr: "format-check failed: 3 files need formatting",
                want: Some("Format check failure"),
                want_line: None,
                want_invalid: None,
            },
            Case {
                stdout: "",
                stderr: "lint check failed with 5 errors",
                want: Some("Lint check failure"),
                want_line: None,
                want_invalid: None,
            },
            Case {
                stdout: "all good",
                stderr: "",
                want: None,
                want_line: None,
                want_invalid: None,
            },
            Case {
                stdout: "build succeeded",
                stderr: "test passed",
                want: None,
                want_line: None,
                want_invalid: None,
            },
            Case {
                stdout: "",
                stderr: "error: something went wrong",
                want: None,
                want_line: None,
                want_invalid: None,
            },
            Case {
                stdout: "",
                stderr: "pyproject.toml:100:5: error",
                want: None,
                want_line: None,
                want_invalid: None,
            },
        ];

        for case in cases {
            let got = detect_ci_error_pattern(case.stdout, case.stderr);
            assert_eq!(
                got.as_ref().map(|p| p.pattern_type),
                case.want,
                "stderr={} stdout={}",
                case.stderr,
                case.stdout
            );
            if let Some(pattern) = got {
                assert_eq!(
                    pattern.line_number, case.want_line,
                    "line_number mismatch for stderr={} stdout={}",
                    case.stderr, case.stdout
                );
                assert_eq!(
                    pattern.invalid_value.as_deref(),
                    case.want_invalid,
                    "invalid_value mismatch for stderr={} stdout={}",
                    case.stderr,
                    case.stdout
                );
            }
        }
    }

    #[test]
    fn detect_toml_takes_precedence_over_unknown_variant() {
        let output = "TOML parse error at line 44: unknown variant `py314`";
        let pattern = detect_ci_error_pattern("", output).unwrap();
        assert_eq!(pattern.pattern_type, "TOML parse error");
        assert_eq!(pattern.line_number, Some(44));
    }

    #[test]
    fn detect_toml_takes_precedence_over_ruff() {
        let output = "ruff failed: TOML parse error at line 50";
        let pattern = detect_ci_error_pattern("", output).unwrap();
        assert_eq!(pattern.pattern_type, "TOML parse error");
        assert_eq!(pattern.line_number, Some(50));
    }

    #[test]
    fn detect_unknown_variant_takes_precedence_over_ruff() {
        let output = "ruff: unknown variant `bad`";
        let pattern = detect_ci_error_pattern("", output).unwrap();
        assert_eq!(pattern.pattern_type, "Unknown variant error");
    }

    #[test]
    fn detect_format_takes_precedence_over_lint_when_both_present() {
        let pattern = detect_ci_error_pattern(
            "format-check failed: 1 file needs formatting",
            "lint check failed with 2 errors",
        )
        .unwrap();
        assert_eq!(pattern.pattern_type, "Format check failure");
    }

    #[test]
    fn extract_valid_values_handles_period_terminator() {
        let output = "expected one of foo, bar, baz.";
        assert_eq!(
            extract_valid_values(output),
            Some("foo, bar, baz".to_string())
        );
    }

    #[test]
    fn extract_valid_values_handles_newline_terminator() {
        let output = "expected one of a, b\nc";
        assert_eq!(extract_valid_values(output), Some("a, b".to_string()));
    }

    #[test]
    fn extract_line_number_handles_comma_suffix() {
        let output = "at line 42, column 10";
        assert_eq!(extract_line_number(output), Some(42));
    }

    #[test]
    fn detect_format_case_insensitive() {
        let output = "FORMAT-CHECK FAILED";
        let pattern = detect_format_check_error(output).unwrap();
        assert_eq!(pattern.pattern_type, "Format check failure");
    }

    #[test]
    fn detect_lint_case_insensitive() {
        let output = "LINT CHECK FAILED";
        let pattern = detect_lint_check_error(output).unwrap();
        assert_eq!(pattern.pattern_type, "Lint check failure");
    }

    #[test]
    fn detect_ruff_yields_to_toml_parse() {
        let output = "ruff failed: TOML parse error";
        let pattern = detect_ruff_error(output);
        assert!(
            pattern.is_none(),
            "ruff detector should yield to TOML parse"
        );
    }
}
