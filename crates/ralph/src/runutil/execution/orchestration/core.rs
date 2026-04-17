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

use anyhow::Result;

use crate::constants::buffers::TIMEOUT_STDOUT_CAPTURE_MAX_BYTES;
use crate::runner;
use crate::runner::{RetryableReason, RunnerFailureClass};

use super::super::super::abort::{RunAbort, RunAbortReason};
use super::super::super::revert::{apply_git_revert_mode, format_revert_failure_message};
use super::super::super::{SeededRng, compute_backoff, format_duration};
use super::super::backend::{
    RealRunnerBackend, RunnerAttemptContext, RunnerBackend, RunnerErrorMessages, RunnerInvocation,
    emit_operation, wrap_output_handler_with_capture,
};
use super::super::retry_policy::should_retry_with_repo_state;
use super::failure_paths::{
    FailureOutcome, FailureRecoveryContext, FailureSessionIds, NonZeroExitDetails,
    handle_non_zero_exit, handle_other_failure, handle_signal_recovery,
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
        settings,
        execution,
        failure,
        retry,
    } = invocation;
    let RunnerErrorMessages {
        log_label,
        interrupted_msg,
        timeout_msg,
        terminated_msg,
        mut non_zero_msg,
        other_msg,
    } = messages;

    let should_capture_timeout_stdout = failure.revert_on_error && settings.timeout.is_some();
    let (timeout_stdout_capture, effective_output_handler) = if should_capture_timeout_stdout {
        let (capture, handler) = wrap_output_handler_with_capture(
            settings.output_handler.clone(),
            TIMEOUT_STDOUT_CAPTURE_MAX_BYTES,
        );
        (Some(capture), handler)
    } else {
        (None, settings.output_handler.clone())
    };

    let mut attempt: u32 = 1;
    let max_attempts = retry.policy.max_attempts;
    let mut rng = SeededRng::new();
    let mut signal_resume_attempts: u8 = 0;
    let attempt_context =
        settings.attempt_context(effective_output_handler.clone(), execution.phase_type);

    emit_operation(
        &effective_output_handler,
        &format!("Running runner attempt {}/ {}", attempt, max_attempts),
    );

    let mut result = run_runner_attempt(
        backend,
        &attempt_context,
        execution.prompt,
        execution.session_id.clone(),
    );

    loop {
        match result {
            Ok(output) => return Ok(output),
            Err(runner::RunnerError::Interrupted) => {
                let message = if failure.revert_on_error {
                    let outcome = apply_git_revert_mode(
                        settings.repo_root,
                        failure.git_revert_mode,
                        log_label,
                        failure.revert_prompt.as_ref(),
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
                let classification = err.classify(&settings.runner_kind);
                if attempt < max_attempts
                    && matches!(classification, RunnerFailureClass::Retryable(_))
                    && should_retry_with_repo_state(
                        settings.repo_root,
                        failure.revert_on_error,
                        failure.git_revert_mode,
                    )?
                {
                    if failure.revert_on_error
                        && failure.git_revert_mode == crate::contracts::GitRevertMode::Enabled
                        && let Err(err) = crate::git::revert_uncommitted(settings.repo_root)
                    {
                        log::warn!("Failed to auto-revert before retry: {}", err);
                    }

                    let delay = compute_backoff(retry.policy, attempt, &mut rng);
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
                        &attempt_context,
                        execution.prompt,
                        execution.session_id.clone(),
                    );
                    continue;
                }

                match result {
                    Ok(_) => unreachable!(),
                    Err(runner::RunnerError::Timeout) => {
                        return Err(handle_timeout_failure(
                            settings.repo_root,
                            failure.git_revert_mode,
                            log_label,
                            failure.revert_prompt.as_ref(),
                            timeout_stdout_capture.as_ref(),
                            failure.revert_on_error,
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
                        &attempt_context,
                        FailureRecoveryContext {
                            git_revert_mode: failure.git_revert_mode,
                            log_label,
                            revert_prompt: failure.revert_prompt.as_ref(),
                            timeout_stdout_capture: timeout_stdout_capture.as_ref(),
                            revert_on_error: failure.revert_on_error,
                        },
                        FailureSessionIds {
                            invocation: execution.session_id.as_deref(),
                            error: error_session_id.as_deref(),
                        },
                        NonZeroExitDetails {
                            code,
                            stdout: &stdout,
                            stderr: &stderr,
                        },
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
                            &mut signal_resume_attempts,
                            signal,
                            &attempt_context,
                            execution.prompt,
                            FailureSessionIds {
                                invocation: execution.session_id.as_deref(),
                                error: error_session_id.as_deref(),
                            },
                        ) {
                            result = next_result;
                            continue;
                        }

                        match handle_terminated_signal_failure(
                            backend,
                            &attempt_context,
                            FailureRecoveryContext {
                                git_revert_mode: failure.git_revert_mode,
                                log_label,
                                revert_prompt: failure.revert_prompt.as_ref(),
                                timeout_stdout_capture: timeout_stdout_capture.as_ref(),
                                revert_on_error: failure.revert_on_error,
                            },
                            FailureSessionIds {
                                invocation: execution.session_id.as_deref(),
                                error: error_session_id.as_deref(),
                            },
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
                            settings.repo_root,
                            failure.git_revert_mode,
                            log_label,
                            failure.revert_prompt.as_ref(),
                            failure.revert_on_error,
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

fn run_runner_attempt(
    backend: &mut impl RunnerBackend,
    attempt_context: &RunnerAttemptContext<'_>,
    prompt: &str,
    session_id: Option<String>,
) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
    backend.run_prompt(attempt_context.run_prompt_request(prompt, session_id))
}
