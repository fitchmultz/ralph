//! CI gate execution for post-run supervision.
//!
//! Purpose:
//! - CI gate execution for post-run supervision.
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
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - CI gate command is configured or defaults to "make ci".
//! - Command output is captured (not inherited) to include in compliance messages.
//! - Error pattern detection is best-effort; undetected patterns fall back to generic guidance.

use super::logging;
#[path = "ci_format.rs"]
mod ci_format;
#[path = "ci_patterns.rs"]
mod ci_patterns;
#[path = "ci_support.rs"]
mod ci_support;

use crate::constants::limits::{CI_FAILURE_ESCALATION_THRESHOLD, CI_GATE_AUTO_RETRY_LIMIT};
use crate::runutil;
use anyhow::{Context, Result, bail};
use ci_format::format_detected_pattern;
#[cfg(test)]
use ci_format::{format_ci_output_for_message, truncate_for_log};
#[cfg(test)]
use ci_patterns::{
    DetectedErrorPattern, detect_format_check_error, detect_lint_check_error,
    detect_lock_contention_error, detect_ruff_error, detect_toml_parse_error,
    detect_unknown_variant_error, extract_invalid_value, extract_line_number, extract_valid_values,
    infer_file_path,
};
use ci_patterns::{LOCK_CONTENTION_GUIDANCE, detect_ci_error_pattern};
pub(crate) use ci_support::CiFailure;
#[cfg(test)]
use ci_support::get_error_pattern_key;
use ci_support::{
    CiGateResult, build_ci_failure_message_with_user_input, ci_gate_result_from_failure,
    strict_ci_gate_compliance_message,
};
use std::time::Instant;

/// Executes the CI gate command if enabled and always returns the captured result.
pub(crate) fn capture_ci_gate_result(resolved: &crate::config::Resolved) -> Result<CiGateResult> {
    let ci_gate = resolved
        .config
        .agent
        .ci_gate
        .as_ref()
        .filter(|ci_gate| ci_gate.is_enabled());
    let Some(ci_gate) = ci_gate else {
        log::info!("CI gate disabled; skipping.");
        return Ok(CiGateResult {
            success: true,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    };

    let command = ci_gate.display_string();

    logging::with_scope(&format!("CI gate ({command})"), || {
        log::info!(
            "CI gate command started (may take several minutes): {}",
            command
        );
        let started = Instant::now();

        let output = runutil::execute_ci_gate(ci_gate, &resolved.repo_root).with_context(|| {
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
        let elapsed = started.elapsed();
        log::info!(
            "CI gate command finished in {:.1}s with exit code {:?}",
            elapsed.as_secs_f64(),
            exit_code
        );

        if !success
            && detect_ci_error_pattern(&stdout, &stderr)
                .as_ref()
                .is_some_and(|pattern| pattern.pattern_type == "Lock contention")
        {
            log::warn!(
                "CI gate failure indicates lock contention. {}",
                LOCK_CONTENTION_GUIDANCE
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

/// Executes the CI gate command if enabled.
///
/// Returns a CiGateResult containing success status, exit code, and captured output.
pub(crate) fn run_ci_gate(resolved: &crate::config::Resolved) -> Result<CiGateResult> {
    let result = capture_ci_gate_result(resolved)?;
    if result.success {
        return Ok(result);
    }

    let detected = detect_ci_error_pattern(&result.stdout, &result.stderr);
    let error_pattern = detected.as_ref().map(|p| p.pattern_type);

    Err(CiFailure {
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
        error_pattern,
    }
    .into())
}

fn send_continue_message<F>(
    resolved: &crate::config::Resolved,
    continue_session: &mut super::ContinueSession,
    message: &str,
    on_resume: &mut F,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<()>
where
    F: FnMut(&crate::runner::RunnerOutput, std::time::Duration) -> Result<()>,
{
    let resumed = super::resume_continue_session(resolved, continue_session, message, plugins)?;
    on_resume(&resumed.output, resumed.elapsed)
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

            let gate_result = ci_gate_result_from_failure(&result);

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

            match outcome {
                runutil::RevertOutcome::Continue { message } => {
                    let combined_message =
                        build_ci_failure_message_with_user_input(resolved, &gate_result, &message);
                    send_continue_message(
                        resolved,
                        continue_session,
                        &combined_message,
                        &mut on_resume,
                        plugins,
                    )?;

                    // User intervention supplied new guidance; give the agent a fresh retry window.
                    continue_session.last_ci_error_pattern = None;
                    continue_session.consecutive_same_error_count = 0;
                    continue_session.ci_failure_retry_count = 0;
                    continue;
                }
                _ => {
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
            }
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
            let gate_result = ci_gate_result_from_failure(&result);
            let message = strict_ci_gate_compliance_message(resolved, &gate_result);
            send_continue_message(
                resolved,
                continue_session,
                &message,
                &mut on_resume,
                plugins,
            )?;
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
                // Prepend strict CI compliance message to ensure agent sees CI output
                let gate_result = ci_gate_result_from_failure(&result);
                let combined_message =
                    build_ci_failure_message_with_user_input(resolved, &gate_result, &message);
                send_continue_message(
                    resolved,
                    continue_session,
                    &combined_message,
                    &mut on_resume,
                    plugins,
                )?;
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
        .ci_gate
        .as_ref()
        .map(|ci_gate| ci_gate.display_string())
        .unwrap_or_else(|| "disabled".to_string())
}

#[cfg(test)]
#[path = "ci_tests.rs"]
mod tests;
