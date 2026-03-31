//! Purpose: Focused runner-orchestration failure handlers.
//!
//! Responsibilities:
//! - Shape timeout, non-zero-exit, signal, and miscellaneous runner failures.
//! - Centralize safeguard-dump capture and revert/continue handling for error paths.
//! - Keep signal recovery isolated from the main orchestration loop.
//!
//! Scope:
//! - Failure-path helpers only.
//! - Retry cadence and backend invocation sequencing remain in `core.rs`.
//!
//! Usage:
//! - Called by `orchestration/core.rs` when a runner attempt fails.
//!
//! Invariants/Assumptions:
//! - Timeout stdout capture is optional and bounded.
//! - Continue-session retries after revert use the same continuation policy as the main runner flow.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::commands::run::PhaseType;
use crate::constants::limits::MAX_SIGNAL_RESUMES;
use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::redaction::RedactedString;
use crate::{fsutil, runner};

use super::super::super::abort::{RunAbort, RunAbortReason};
use super::super::super::revert::{
    RevertOutcome, RevertPromptHandler, RevertSource, apply_git_revert_mode,
    format_revert_failure_message,
};
use super::super::backend::{RunnerBackend, emit_operation, log_stderr_tail};
use super::super::continue_session::continue_or_rerun;

pub(super) type BackendResult = anyhow::Result<runner::RunnerOutput, runner::RunnerError>;

pub(super) enum FailureOutcome {
    Continue(BackendResult),
    Abort(anyhow::Error),
}

pub(super) fn handle_timeout_failure(
    repo_root: &Path,
    git_revert_mode: GitRevertMode,
    log_label: &str,
    revert_prompt: Option<&RevertPromptHandler>,
    timeout_stdout_capture: Option<&Arc<Mutex<String>>>,
    revert_on_error: bool,
    timeout_msg: &str,
) -> Result<anyhow::Error> {
    let safeguard_msg = if revert_on_error {
        capture_timeout_stdout_safeguard(timeout_stdout_capture)
    } else {
        String::new()
    };

    if !revert_on_error {
        return Ok(anyhow!("{}{}", timeout_msg, safeguard_msg));
    }

    let outcome = apply_git_revert_mode(repo_root, git_revert_mode, log_label, revert_prompt)?;
    let message = format_revert_failure_message(timeout_msg, outcome.clone());
    Ok(match outcome {
        RevertOutcome::Reverted {
            source: RevertSource::User,
        } => anyhow::Error::new(RunAbort::new(
            RunAbortReason::UserRevert,
            format!("{}{}", message, safeguard_msg),
        )),
        _ => anyhow!("{}{}", message, safeguard_msg),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_non_zero_exit<FNonZero>(
    backend: &mut impl RunnerBackend,
    runner_kind: &Runner,
    repo_root: &Path,
    bins: runner::RunnerBinaries<'_>,
    model: &Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: runner::ResolvedRunnerCliOptions,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    phase_type: PhaseType,
    invocation_session_id: Option<&str>,
    error_session_id: Option<&str>,
    git_revert_mode: GitRevertMode,
    log_label: &str,
    revert_prompt: Option<&RevertPromptHandler>,
    timeout_stdout_capture: Option<&Arc<Mutex<String>>>,
    revert_on_error: bool,
    code: i32,
    stdout: &RedactedString,
    stderr: &RedactedString,
    non_zero_msg: &mut FNonZero,
) -> Result<FailureOutcome>
where
    FNonZero: FnMut(i32) -> String,
{
    log_stderr_tail(log_label, &stderr.to_string());
    let base_msg = non_zero_msg(code);
    let safeguard_msg = if revert_on_error {
        capture_stdio_safeguards(stdout, stderr)
    } else {
        String::new()
    };

    handle_revertable_failure(
        backend,
        runner_kind,
        repo_root,
        bins,
        model,
        reasoning_effort,
        runner_cli,
        timeout,
        permission_mode,
        output_handler,
        output_stream,
        phase_type,
        invocation_session_id,
        error_session_id,
        git_revert_mode,
        log_label,
        revert_prompt,
        timeout_stdout_capture,
        revert_on_error,
        &base_msg,
        safeguard_msg,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_signal_recovery(
    backend: &mut impl RunnerBackend,
    output_handler: &Option<runner::OutputHandler>,
    signal_resume_attempts: &mut u8,
    signal: Option<i32>,
    runner_kind: &Runner,
    repo_root: &Path,
    bins: runner::RunnerBinaries<'_>,
    model: &Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: runner::ResolvedRunnerCliOptions,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_stream: runner::OutputStream,
    phase_type: PhaseType,
    invocation_session_id: Option<&str>,
    error_session_id: Option<&str>,
) -> Option<BackendResult> {
    if *signal_resume_attempts >= MAX_SIGNAL_RESUMES {
        return None;
    }

    *signal_resume_attempts = signal_resume_attempts.saturating_add(1);
    let signal_label = signal
        .map(|signal| signal.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    emit_operation(
        output_handler,
        &format!(
            "Runner signal recovery {}/{} (signal={})",
            signal_resume_attempts, MAX_SIGNAL_RESUMES, signal_label
        ),
    );
    let continue_message = format!(
        "The previous run was interrupted by signal {}. Continue the task from where you left off. If no prior progress exists, restart from the beginning and complete the task.",
        signal_label
    );
    Some(continue_or_rerun(
        backend,
        runner_kind,
        repo_root,
        bins,
        model,
        reasoning_effort,
        runner_cli,
        &continue_message,
        prompt,
        timeout,
        permission_mode,
        output_handler.clone(),
        output_stream,
        phase_type,
        invocation_session_id,
        error_session_id,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_terminated_signal_failure(
    backend: &mut impl RunnerBackend,
    runner_kind: &Runner,
    repo_root: &Path,
    bins: runner::RunnerBinaries<'_>,
    model: &Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: runner::ResolvedRunnerCliOptions,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    phase_type: PhaseType,
    invocation_session_id: Option<&str>,
    error_session_id: Option<&str>,
    git_revert_mode: GitRevertMode,
    log_label: &str,
    revert_prompt: Option<&RevertPromptHandler>,
    timeout_stdout_capture: Option<&Arc<Mutex<String>>>,
    revert_on_error: bool,
    terminated_msg: &str,
    stdout: &RedactedString,
    stderr: &RedactedString,
) -> Result<FailureOutcome> {
    log_stderr_tail(log_label, &stderr.to_string());
    let safeguard_msg = if revert_on_error {
        capture_stdio_safeguards(stdout, stderr)
    } else {
        String::new()
    };

    handle_revertable_failure(
        backend,
        runner_kind,
        repo_root,
        bins,
        model,
        reasoning_effort,
        runner_cli,
        timeout,
        permission_mode,
        output_handler,
        output_stream,
        phase_type,
        invocation_session_id,
        error_session_id,
        git_revert_mode,
        log_label,
        revert_prompt,
        timeout_stdout_capture,
        revert_on_error,
        terminated_msg,
        safeguard_msg,
    )
}

pub(super) fn handle_other_failure(
    repo_root: &Path,
    git_revert_mode: GitRevertMode,
    log_label: &str,
    revert_prompt: Option<&RevertPromptHandler>,
    revert_on_error: bool,
    base_msg: String,
) -> Result<anyhow::Error> {
    if !revert_on_error {
        return Ok(anyhow!(base_msg));
    }

    let outcome = apply_git_revert_mode(repo_root, git_revert_mode, log_label, revert_prompt)?;
    let message = format_revert_failure_message(&base_msg, outcome.clone());
    Ok(match outcome {
        RevertOutcome::Reverted {
            source: RevertSource::User,
        } => anyhow::Error::new(RunAbort::new(RunAbortReason::UserRevert, message)),
        _ => anyhow!(message),
    })
}

#[allow(clippy::too_many_arguments)]
fn handle_revertable_failure(
    backend: &mut impl RunnerBackend,
    runner_kind: &Runner,
    repo_root: &Path,
    bins: runner::RunnerBinaries<'_>,
    model: &Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: runner::ResolvedRunnerCliOptions,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    phase_type: PhaseType,
    invocation_session_id: Option<&str>,
    error_session_id: Option<&str>,
    git_revert_mode: GitRevertMode,
    log_label: &str,
    revert_prompt: Option<&RevertPromptHandler>,
    timeout_stdout_capture: Option<&Arc<Mutex<String>>>,
    revert_on_error: bool,
    base_msg: &str,
    safeguard_msg: String,
) -> Result<FailureOutcome> {
    if !revert_on_error {
        return Ok(FailureOutcome::Abort(anyhow!(
            "{}{}",
            base_msg,
            safeguard_msg
        )));
    }

    let outcome = apply_git_revert_mode(repo_root, git_revert_mode, log_label, revert_prompt)?;
    match outcome {
        RevertOutcome::Continue { message } => {
            clear_timeout_capture(timeout_stdout_capture);
            Ok(FailureOutcome::Continue(continue_or_rerun(
                backend,
                runner_kind,
                repo_root,
                bins,
                model,
                reasoning_effort,
                runner_cli,
                &message,
                &message,
                timeout,
                permission_mode,
                output_handler,
                output_stream,
                phase_type,
                invocation_session_id,
                error_session_id,
            )))
        }
        RevertOutcome::Reverted {
            source: RevertSource::User,
        } => {
            let message = format_revert_failure_message(base_msg, outcome);
            Ok(FailureOutcome::Abort(anyhow::Error::new(RunAbort::new(
                RunAbortReason::UserRevert,
                format!("{}{}", message, safeguard_msg),
            ))))
        }
        _ => {
            let message = format_revert_failure_message(base_msg, outcome);
            Ok(FailureOutcome::Abort(anyhow!(
                "{}{}",
                message,
                safeguard_msg
            )))
        }
    }
}

fn capture_timeout_stdout_safeguard(timeout_stdout_capture: Option<&Arc<Mutex<String>>>) -> String {
    let Some(capture) = timeout_stdout_capture else {
        return String::new();
    };

    let captured = match capture.lock() {
        Ok(buf) => buf.clone(),
        Err(poisoned) => {
            log::warn!(
                "timeout_stdout_capture mutex poisoned; recovering captured output for diagnostics"
            );
            poisoned.into_inner().clone()
        }
    };
    if captured.trim().is_empty() {
        return String::new();
    }

    match fsutil::safeguard_text_dump_redacted("runner_error", &captured) {
        Ok(path) => format!("\n(redacted output saved to {})", path.display()),
        Err(err) => {
            log::warn!("failed to save safeguard dump: {}", err);
            String::new()
        }
    }
}

fn capture_stdio_safeguards(stdout: &RedactedString, stderr: &RedactedString) -> String {
    let mut safeguard_msg = String::new();

    if !stdout.0.is_empty() {
        match fsutil::safeguard_text_dump_redacted("runner_error_stdout", &stdout.to_string()) {
            Ok(path) => {
                safeguard_msg = format!("\n(redacted stdout saved to {})", path.display());
            }
            Err(err) => log::warn!("failed to save stdout safeguard dump: {}", err),
        }
    }

    if !stderr.0.is_empty() {
        match fsutil::safeguard_text_dump_redacted("runner_error_stderr", &stderr.to_string()) {
            Ok(path) => {
                safeguard_msg.push_str(&format!("\n(redacted stderr saved to {})", path.display()))
            }
            Err(err) => log::warn!("failed to save stderr safeguard dump: {}", err),
        }
    }

    safeguard_msg
}

fn clear_timeout_capture(timeout_stdout_capture: Option<&Arc<Mutex<String>>>) {
    if let Some(capture) = timeout_stdout_capture
        && let Ok(mut buf) = capture.lock()
    {
        buf.clear();
    }
}
