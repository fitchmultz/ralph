//! CI gate support types and compliance-message helpers.
//!
//! Purpose:
//! - CI gate support types and compliance-message helpers.
//!
//! Responsibilities:
//! - Define CI result and failure types shared by supervision and tests.
//! - Build operator-facing compliance messages from captured CI output.
//! - Provide stable test helpers for detected CI error patterns.
//!
//! Not handled here:
//! - Executing CI commands.
//! - Continue-session retry control flow.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - CI failures retain stdout/stderr for later compliance messaging.
//! - Compliance messages always include captured output context.

use super::ci_format::{format_ci_output_for_message, format_detected_pattern, truncate_for_log};
use super::ci_patterns::detect_ci_error_pattern;

/// Get a stable key representing the error pattern for comparison.
/// Returns None if no pattern detected, or Some(pattern_type) if detected.
#[cfg(test)]
pub(super) fn get_error_pattern_key(result: &CiGateResult) -> Option<String> {
    detect_ci_error_pattern(&result.stdout, &result.stderr).map(|p| p.pattern_type.to_string())
}

/// Result of running the CI gate command.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct CiGateResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// CI gate failure with captured output for logging.
#[derive(Debug)]
pub(crate) struct CiFailure {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub error_pattern: Option<&'static str>,
}

impl CiFailure {
    pub(crate) fn blocking_state(&self) -> crate::contracts::BlockingState {
        crate::contracts::BlockingState::ci_blocked(
            self.exit_code,
            self.error_pattern.map(str::to_string),
        )
        .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback())
    }
}

impl std::fmt::Display for CiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let exit_code = self.exit_code.unwrap_or(-1);
        write!(f, "CI failed with exit code {}", exit_code)?;

        if let Some(pattern) = &self.error_pattern {
            write!(f, " [{}]", pattern)?;
        }

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

/// Build a combined CI failure message that includes CI output context.
pub(super) fn build_ci_failure_message_with_user_input(
    resolved: &crate::config::Resolved,
    result: &CiGateResult,
    user_message: &str,
) -> String {
    let strict_message = strict_ci_gate_compliance_message(resolved, result);
    if user_message.trim().is_empty() {
        return strict_message;
    }
    format!(
        "{}\n\n---\n\nAgent message from user intervention:\n{}",
        strict_message, user_message
    )
}

pub(super) fn strict_ci_gate_compliance_message(
    resolved: &crate::config::Resolved,
    result: &CiGateResult,
) -> String {
    let cmd = super::ci_gate_command_label(resolved);
    let output_snippet = format_ci_output_for_message(&result.stdout, &result.stderr, 50, 50);
    let exit_code_display = result.exit_code.unwrap_or(-1);
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

pub(super) fn ci_gate_result_from_failure(result: &CiFailure) -> CiGateResult {
    CiGateResult {
        success: false,
        exit_code: result.exit_code,
        stdout: result.stdout.clone(),
        stderr: result.stderr.clone(),
    }
}
