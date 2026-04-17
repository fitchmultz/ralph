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

use anyhow::{Result, anyhow};

use crate::constants::limits::MAX_SIGNAL_RESUMES;
use crate::contracts::GitRevertMode;
use crate::redaction::RedactedString;
use crate::{fsutil, runner};

use super::super::super::abort::{RunAbort, RunAbortReason};
use super::super::super::revert::{
    RevertOutcome, RevertPromptHandler, RevertSource, apply_git_revert_mode,
    format_revert_failure_message,
};
use super::super::backend::{RunnerAttemptContext, RunnerBackend, emit_operation, log_stderr_tail};
use super::super::continue_session::continue_or_rerun;

pub(super) type BackendResult = anyhow::Result<runner::RunnerOutput, runner::RunnerError>;

pub(super) enum FailureOutcome {
    Continue(BackendResult),
    Abort(anyhow::Error),
}

#[derive(Clone, Copy)]
pub(super) struct FailureRecoveryContext<'a> {
    pub git_revert_mode: GitRevertMode,
    pub log_label: &'a str,
    pub revert_prompt: Option<&'a RevertPromptHandler>,
    pub timeout_stdout_capture: Option<&'a Arc<Mutex<String>>>,
    pub revert_on_error: bool,
}

#[derive(Clone, Copy)]
pub(super) struct FailureSessionIds<'a> {
    pub invocation: Option<&'a str>,
    pub error: Option<&'a str>,
}

#[derive(Clone, Copy)]
pub(super) struct NonZeroExitDetails<'a> {
    pub code: i32,
    pub stdout: &'a RedactedString,
    pub stderr: &'a RedactedString,
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

pub(super) fn handle_non_zero_exit<FNonZero>(
    backend: &mut impl RunnerBackend,
    attempt: &RunnerAttemptContext<'_>,
    recovery: FailureRecoveryContext<'_>,
    sessions: FailureSessionIds<'_>,
    details: NonZeroExitDetails<'_>,
    non_zero_msg: &mut FNonZero,
) -> Result<FailureOutcome>
where
    FNonZero: FnMut(i32) -> String,
{
    log_stderr_tail(recovery.log_label, &details.stderr.to_string());
    let base_msg = non_zero_msg(details.code);
    let safeguard_msg = if recovery.revert_on_error {
        capture_stdio_safeguards(details.stdout, details.stderr)
    } else {
        String::new()
    };

    handle_revertable_failure(
        backend,
        attempt,
        recovery,
        sessions,
        &base_msg,
        safeguard_msg,
    )
}

pub(super) fn handle_signal_recovery(
    backend: &mut impl RunnerBackend,
    signal_resume_attempts: &mut u8,
    signal: Option<i32>,
    attempt: &RunnerAttemptContext<'_>,
    prompt: &str,
    sessions: FailureSessionIds<'_>,
) -> Option<BackendResult> {
    if *signal_resume_attempts >= MAX_SIGNAL_RESUMES {
        return None;
    }

    *signal_resume_attempts = signal_resume_attempts.saturating_add(1);
    let signal_label = signal
        .map(|signal| signal.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    emit_operation(
        &attempt.output_handler,
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
        attempt,
        &continue_message,
        prompt,
        sessions.invocation,
        sessions.error,
    ))
}

pub(super) fn handle_terminated_signal_failure(
    backend: &mut impl RunnerBackend,
    attempt: &RunnerAttemptContext<'_>,
    recovery: FailureRecoveryContext<'_>,
    sessions: FailureSessionIds<'_>,
    terminated_msg: &str,
    stdout: &RedactedString,
    stderr: &RedactedString,
) -> Result<FailureOutcome> {
    log_stderr_tail(recovery.log_label, &stderr.to_string());
    let safeguard_msg = if recovery.revert_on_error {
        capture_stdio_safeguards(stdout, stderr)
    } else {
        String::new()
    };

    handle_revertable_failure(
        backend,
        attempt,
        recovery,
        sessions,
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

fn handle_revertable_failure(
    backend: &mut impl RunnerBackend,
    attempt: &RunnerAttemptContext<'_>,
    recovery: FailureRecoveryContext<'_>,
    sessions: FailureSessionIds<'_>,
    base_msg: &str,
    safeguard_msg: String,
) -> Result<FailureOutcome> {
    if !recovery.revert_on_error {
        return Ok(FailureOutcome::Abort(anyhow!(
            "{}{}",
            base_msg,
            safeguard_msg
        )));
    }

    let outcome = apply_git_revert_mode(
        attempt.repo_root,
        recovery.git_revert_mode,
        recovery.log_label,
        recovery.revert_prompt,
    )?;
    match outcome {
        RevertOutcome::Continue { message } => {
            clear_timeout_capture(recovery.timeout_stdout_capture);
            Ok(FailureOutcome::Continue(continue_or_rerun(
                backend,
                attempt,
                &message,
                &message,
                sessions.invocation,
                sessions.error,
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
