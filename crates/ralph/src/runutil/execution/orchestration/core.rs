//! Purpose: Runner execution state machine implementation.
//!
//! Responsibilities:
//! - Execute runner invocations via a backend.
//! - Apply retry, continue-session, revert, and error-shaping policy consistently.
//!
//! Scope:
//! - Main orchestration logic only.
//! - Backend wiring details and focused failure handlers live in sibling companion modules.
//!
//! Usage:
//! - Re-exported by `orchestration/mod.rs` for callers using `crate::runutil::execution`.
//!
//! Invariants/Assumptions:
//! - Timeout safeguard capture is bounded.
//! - Interruptions never retry.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::commands::run::PhaseType;
use crate::constants::buffers::TIMEOUT_STDOUT_CAPTURE_MAX_BYTES;
use crate::contracts::{ClaudePermissionMode, Model, ReasoningEffort, Runner};
use crate::runner;
use crate::runner::{RetryableReason, RunnerFailureClass};

use super::super::super::abort::{RunAbort, RunAbortReason};
use super::super::super::revert::{apply_git_revert_mode, format_revert_failure_message};
use super::super::super::{SeededRng, compute_backoff, format_duration};
use super::super::backend::{
    RealRunnerBackend, RunnerBackend, RunnerErrorMessages, RunnerInvocation, emit_operation,
    wrap_output_handler_with_capture,
};
use super::super::retry_policy::should_retry_with_repo_state;
use super::failure_paths::{
    FailureOutcome, handle_non_zero_exit, handle_other_failure, handle_signal_recovery,
    handle_terminated_signal_failure, handle_timeout_failure,
};

pub(crate) fn run_prompt_with_handling_backend<FNonZero, FOther>(
    invocation: RunnerInvocation<'_>,
    messages: RunnerErrorMessages<'_, FNonZero, FOther>,
    backend: &mut impl RunnerBackend,
) -> Result<runner::RunnerOutput>
where
    FNonZero: FnMut(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    let RunnerInvocation {
        repo_root,
        runner_kind,
        bins,
        model,
        reasoning_effort,
        runner_cli,
        prompt,
        timeout,
        permission_mode,
        revert_on_error,
        git_revert_mode,
        output_handler,
        output_stream,
        revert_prompt,
        phase_type,
        session_id: invocation_session_id,
        retry_policy,
    } = invocation;
    let RunnerErrorMessages {
        log_label,
        interrupted_msg,
        timeout_msg,
        terminated_msg,
        mut non_zero_msg,
        other_msg,
    } = messages;

    let should_capture_timeout_stdout = revert_on_error && timeout.is_some();
    let (timeout_stdout_capture, effective_output_handler) = if should_capture_timeout_stdout {
        let (capture, handler) =
            wrap_output_handler_with_capture(output_handler, TIMEOUT_STDOUT_CAPTURE_MAX_BYTES);
        (Some(capture), handler)
    } else {
        (None, output_handler)
    };

    let mut attempt: u32 = 1;
    let max_attempts = retry_policy.max_attempts;
    let mut rng = SeededRng::new();
    let mut signal_resume_attempts: u8 = 0;

    emit_operation(
        &effective_output_handler,
        &format!("Running runner attempt {}/ {}", attempt, max_attempts),
    );

    let mut result = run_runner_attempt(
        backend,
        &runner_kind,
        repo_root,
        bins,
        &model,
        reasoning_effort,
        runner_cli,
        prompt,
        timeout,
        permission_mode,
        effective_output_handler.clone(),
        output_stream,
        phase_type,
        invocation_session_id.clone(),
    );

    loop {
        match result {
            Ok(output) => return Ok(output),
            Err(runner::RunnerError::Interrupted) => {
                let message = if revert_on_error {
                    let outcome = apply_git_revert_mode(
                        repo_root,
                        git_revert_mode,
                        log_label,
                        revert_prompt.as_ref(),
                    )?;
                    format_revert_failure_message(interrupted_msg, outcome)
                } else {
                    interrupted_msg.to_string()
                };
                return Err(anyhow::Error::new(RunAbort::new(
                    RunAbortReason::Interrupted,
                    message,
                )));
            }
            Err(ref err) => {
                let classification = err.classify(&runner_kind);
                if attempt < max_attempts
                    && matches!(classification, RunnerFailureClass::Retryable(_))
                    && should_retry_with_repo_state(repo_root, revert_on_error, git_revert_mode)?
                {
                    if revert_on_error
                        && git_revert_mode == crate::contracts::GitRevertMode::Enabled
                        && let Err(err) = crate::git::revert_uncommitted(repo_root)
                    {
                        log::warn!("Failed to auto-revert before retry: {}", err);
                    }

                    let delay = compute_backoff(retry_policy, attempt, &mut rng);
                    let reason_str = match classification {
                        RunnerFailureClass::Retryable(RetryableReason::RateLimited) => "rate limit",
                        RunnerFailureClass::Retryable(RetryableReason::TemporaryUnavailable) => {
                            "temporarily unavailable"
                        }
                        RunnerFailureClass::Retryable(RetryableReason::TransientIo) => {
                            "transient error"
                        }
                        _ => "transient error",
                    };

                    emit_operation(
                        &effective_output_handler,
                        &format!(
                            "Runner retry {}/{} in {} ({})",
                            attempt + 1,
                            max_attempts,
                            format_duration(delay),
                            reason_str
                        ),
                    );

                    if let Ok(ctrlc) = runner::ctrlc_state() {
                        if super::super::super::shell::sleep_with_cancellation(
                            delay,
                            Some(&ctrlc.interrupted),
                        )
                        .is_err()
                        {
                            return Err(anyhow::Error::new(RunAbort::new(
                                RunAbortReason::Interrupted,
                                interrupted_msg.to_string(),
                            )));
                        }
                    } else {
                        std::thread::sleep(delay);
                    }

                    attempt += 1;
                    emit_operation(
                        &effective_output_handler,
                        &format!("Running runner attempt {}/ {}", attempt, max_attempts),
                    );
                    result = run_runner_attempt(
                        backend,
                        &runner_kind,
                        repo_root,
                        bins,
                        &model,
                        reasoning_effort,
                        runner_cli,
                        prompt,
                        timeout,
                        permission_mode,
                        effective_output_handler.clone(),
                        output_stream,
                        phase_type,
                        invocation_session_id.clone(),
                    );
                    continue;
                }

                match result {
                    Ok(_) => unreachable!(),
                    Err(runner::RunnerError::Timeout) => {
                        return Err(handle_timeout_failure(
                            repo_root,
                            git_revert_mode,
                            log_label,
                            revert_prompt.as_ref(),
                            timeout_stdout_capture.as_ref(),
                            revert_on_error,
                            timeout_msg,
                        )?);
                    }
                    Err(runner::RunnerError::NonZeroExit {
                        code,
                        stdout,
                        stderr,
                        session_id: error_session_id,
                    }) => match handle_non_zero_exit(
                        backend,
                        &runner_kind,
                        repo_root,
                        bins,
                        &model,
                        reasoning_effort,
                        runner_cli,
                        timeout,
                        permission_mode,
                        effective_output_handler.clone(),
                        output_stream,
                        phase_type,
                        invocation_session_id.as_deref(),
                        error_session_id.as_deref(),
                        git_revert_mode,
                        log_label,
                        revert_prompt.as_ref(),
                        timeout_stdout_capture.as_ref(),
                        revert_on_error,
                        code,
                        &stdout,
                        &stderr,
                        &mut non_zero_msg,
                    )? {
                        FailureOutcome::Continue(next_result) => {
                            result = next_result;
                            continue;
                        }
                        FailureOutcome::Abort(err) => return Err(err),
                    },
                    Err(runner::RunnerError::TerminatedBySignal {
                        signal,
                        stdout,
                        stderr,
                        session_id: error_session_id,
                    }) => {
                        if let Some(next_result) = handle_signal_recovery(
                            backend,
                            &effective_output_handler,
                            &mut signal_resume_attempts,
                            signal,
                            &runner_kind,
                            repo_root,
                            bins,
                            &model,
                            reasoning_effort,
                            runner_cli,
                            prompt,
                            timeout,
                            permission_mode,
                            output_stream,
                            phase_type,
                            invocation_session_id.as_deref(),
                            error_session_id.as_deref(),
                        ) {
                            result = next_result;
                            continue;
                        }

                        match handle_terminated_signal_failure(
                            backend,
                            &runner_kind,
                            repo_root,
                            bins,
                            &model,
                            reasoning_effort,
                            runner_cli,
                            timeout,
                            permission_mode,
                            effective_output_handler.clone(),
                            output_stream,
                            phase_type,
                            invocation_session_id.as_deref(),
                            error_session_id.as_deref(),
                            git_revert_mode,
                            log_label,
                            revert_prompt.as_ref(),
                            timeout_stdout_capture.as_ref(),
                            revert_on_error,
                            terminated_msg,
                            &stdout,
                            &stderr,
                        )? {
                            FailureOutcome::Continue(next_result) => {
                                result = next_result;
                                continue;
                            }
                            FailureOutcome::Abort(err) => return Err(err),
                        }
                    }
                    Err(err) => {
                        return Err(handle_other_failure(
                            repo_root,
                            git_revert_mode,
                            log_label,
                            revert_prompt.as_ref(),
                            revert_on_error,
                            other_msg(err),
                        )?);
                    }
                }
            }
        }
    }
}

pub(crate) fn run_prompt_with_handling<FNonZero, FOther>(
    invocation: RunnerInvocation<'_>,
    messages: RunnerErrorMessages<'_, FNonZero, FOther>,
) -> Result<runner::RunnerOutput>
where
    FNonZero: FnMut(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    let mut backend = RealRunnerBackend;
    run_prompt_with_handling_backend(invocation, messages, &mut backend)
}

#[allow(clippy::too_many_arguments)]
fn run_runner_attempt(
    backend: &mut impl RunnerBackend,
    runner_kind: &Runner,
    repo_root: &Path,
    bins: runner::RunnerBinaries<'_>,
    model: &Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: runner::ResolvedRunnerCliOptions,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    phase_type: PhaseType,
    session_id: Option<String>,
) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
    backend.run_prompt(
        runner_kind.clone(),
        repo_root,
        bins,
        model.clone(),
        reasoning_effort,
        runner_cli,
        prompt,
        timeout,
        permission_mode,
        output_handler,
        output_stream,
        phase_type,
        session_id,
        None,
    )
}
