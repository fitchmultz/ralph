//! Purpose: Runner execution state machine implementation.
//!
//! Responsibilities:
//! - Execute runner invocations via a backend.
//! - Apply retry, continue-session, revert, and error-shaping policy consistently.
//!
//! Scope:
//! - Main orchestration logic only.
//! - Backend wiring details and regression coverage live in sibling companion modules.
//!
//! Usage:
//! - Re-exported by `orchestration/mod.rs` for callers using `crate::runutil::execution`.
//!
//! Invariants/Assumptions:
//! - Timeout safeguard capture is bounded.
//! - Interruptions never retry.

use anyhow::{Result, bail};

use crate::constants::buffers::TIMEOUT_STDOUT_CAPTURE_MAX_BYTES;
use crate::constants::limits::MAX_SIGNAL_RESUMES;
use crate::runner::{RetryableReason, RunnerFailureClass};
use crate::{fsutil, runner};

use super::super::super::abort::{RunAbort, RunAbortReason};
use super::super::super::revert::{
    RevertOutcome, RevertSource, apply_git_revert_mode, format_revert_failure_message,
};
use super::super::super::{SeededRng, compute_backoff, format_duration};
use super::super::backend::{
    RealRunnerBackend, RunnerBackend, RunnerErrorMessages, RunnerInvocation, emit_operation,
    log_stderr_tail, wrap_output_handler_with_capture,
};
use super::super::continue_session::continue_or_rerun;
use super::super::retry_policy::should_retry_with_repo_state;

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

    let mut result = backend.run_prompt(
        runner_kind.clone(),
        repo_root,
        bins,
        model.clone(),
        reasoning_effort,
        runner_cli,
        prompt,
        timeout,
        permission_mode,
        effective_output_handler.clone(),
        output_stream,
        phase_type,
        invocation_session_id.clone(),
        None,
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
                    result = backend.run_prompt(
                        runner_kind.clone(),
                        repo_root,
                        bins,
                        model.clone(),
                        reasoning_effort,
                        runner_cli,
                        prompt,
                        timeout,
                        permission_mode,
                        effective_output_handler.clone(),
                        output_stream,
                        phase_type,
                        invocation_session_id.clone(),
                        None,
                    );
                    continue;
                }

                match result {
                    Ok(_) => unreachable!(),
                    Err(runner::RunnerError::Timeout) => {
                        let mut safeguard_msg = String::new();
                        let message = if revert_on_error {
                            if let Some(capture) = timeout_stdout_capture.as_ref() {
                                let captured = match capture.lock() {
                                    Ok(buf) => buf.clone(),
                                    Err(poisoned) => {
                                        log::warn!(
                                            "timeout_stdout_capture mutex poisoned; recovering captured output for diagnostics"
                                        );
                                        poisoned.into_inner().clone()
                                    }
                                };
                                if !captured.trim().is_empty() {
                                    match fsutil::safeguard_text_dump_redacted(
                                        "runner_error",
                                        &captured,
                                    ) {
                                        Ok(path) => {
                                            safeguard_msg = format!(
                                                "\n(redacted output saved to {})",
                                                path.display()
                                            );
                                        }
                                        Err(err) => {
                                            log::warn!("failed to save safeguard dump: {}", err)
                                        }
                                    }
                                }
                            }

                            let outcome = apply_git_revert_mode(
                                repo_root,
                                git_revert_mode,
                                log_label,
                                revert_prompt.as_ref(),
                            )?;
                            if matches!(
                                outcome,
                                RevertOutcome::Reverted {
                                    source: RevertSource::User
                                }
                            ) {
                                let message = format_revert_failure_message(timeout_msg, outcome);
                                return Err(anyhow::Error::new(RunAbort::new(
                                    RunAbortReason::UserRevert,
                                    format!("{}{}", message, safeguard_msg),
                                )));
                            }
                            format_revert_failure_message(timeout_msg, outcome)
                        } else {
                            timeout_msg.to_string()
                        };

                        bail!("{}{}", message, safeguard_msg);
                    }
                    Err(runner::RunnerError::NonZeroExit {
                        code,
                        stdout,
                        stderr,
                        session_id: error_session_id,
                    }) => {
                        log_stderr_tail(log_label, &stderr.to_string());
                        let base_msg = non_zero_msg(code);
                        let mut safeguard_msg = String::new();
                        if revert_on_error {
                            if !stdout.0.is_empty() {
                                match fsutil::safeguard_text_dump_redacted(
                                    "runner_error_stdout",
                                    &stdout.to_string(),
                                ) {
                                    Ok(path) => {
                                        safeguard_msg = format!(
                                            "\n(redacted stdout saved to {})",
                                            path.display()
                                        );
                                    }
                                    Err(err) => {
                                        log::warn!("failed to save stdout safeguard dump: {}", err)
                                    }
                                }
                            }
                            if !stderr.0.is_empty() {
                                match fsutil::safeguard_text_dump_redacted(
                                    "runner_error_stderr",
                                    &stderr.to_string(),
                                ) {
                                    Ok(path) => safeguard_msg.push_str(&format!(
                                        "\n(redacted stderr saved to {})",
                                        path.display()
                                    )),
                                    Err(err) => {
                                        log::warn!("failed to save stderr safeguard dump: {}", err)
                                    }
                                }
                            }
                            let outcome = apply_git_revert_mode(
                                repo_root,
                                git_revert_mode,
                                log_label,
                                revert_prompt.as_ref(),
                            )?;
                            match outcome {
                                RevertOutcome::Continue { message } => {
                                    if let Some(capture) = timeout_stdout_capture.as_ref()
                                        && let Ok(mut buf) = capture.lock()
                                    {
                                        buf.clear();
                                    }
                                    result = continue_or_rerun(
                                        backend,
                                        &runner_kind,
                                        repo_root,
                                        bins,
                                        &model,
                                        reasoning_effort,
                                        runner_cli,
                                        &message,
                                        &message,
                                        timeout,
                                        permission_mode,
                                        effective_output_handler.clone(),
                                        output_stream,
                                        phase_type,
                                        invocation_session_id.as_deref(),
                                        error_session_id.as_deref(),
                                    );
                                    continue;
                                }
                                RevertOutcome::Reverted {
                                    source: RevertSource::User,
                                } => {
                                    let message = format_revert_failure_message(&base_msg, outcome);
                                    return Err(anyhow::Error::new(RunAbort::new(
                                        RunAbortReason::UserRevert,
                                        format!("{}{}", message, safeguard_msg),
                                    )));
                                }
                                _ => {
                                    let message = format_revert_failure_message(&base_msg, outcome);
                                    bail!("{}{}", message, safeguard_msg);
                                }
                            }
                        }
                        bail!("{}{}", base_msg, safeguard_msg);
                    }
                    Err(runner::RunnerError::TerminatedBySignal {
                        signal,
                        stdout,
                        stderr,
                        session_id: error_session_id,
                    }) => {
                        if signal_resume_attempts < MAX_SIGNAL_RESUMES {
                            signal_resume_attempts = signal_resume_attempts.saturating_add(1);
                            let signal_label = signal
                                .map(|signal| signal.to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            emit_operation(
                                &effective_output_handler,
                                &format!(
                                    "Runner signal recovery {}/{} (signal={})",
                                    signal_resume_attempts, MAX_SIGNAL_RESUMES, signal_label
                                ),
                            );
                            let continue_message = format!(
                                "The previous run was interrupted by signal {}. Continue the task from where you left off. If no prior progress exists, restart from the beginning and complete the task.",
                                signal_label
                            );
                            result = continue_or_rerun(
                                backend,
                                &runner_kind,
                                repo_root,
                                bins,
                                &model,
                                reasoning_effort,
                                runner_cli,
                                &continue_message,
                                prompt,
                                timeout,
                                permission_mode,
                                effective_output_handler.clone(),
                                output_stream,
                                phase_type,
                                invocation_session_id.as_deref(),
                                error_session_id.as_deref(),
                            );
                            continue;
                        }

                        log_stderr_tail(log_label, &stderr.to_string());
                        let mut safeguard_msg = String::new();
                        if revert_on_error {
                            if !stdout.0.is_empty() {
                                match fsutil::safeguard_text_dump_redacted(
                                    "runner_error_stdout",
                                    &stdout.to_string(),
                                ) {
                                    Ok(path) => {
                                        safeguard_msg = format!(
                                            "\n(redacted stdout saved to {})",
                                            path.display()
                                        );
                                    }
                                    Err(err) => {
                                        log::warn!("failed to save stdout safeguard dump: {}", err)
                                    }
                                }
                            }
                            if !stderr.0.is_empty() {
                                match fsutil::safeguard_text_dump_redacted(
                                    "runner_error_stderr",
                                    &stderr.to_string(),
                                ) {
                                    Ok(path) => safeguard_msg.push_str(&format!(
                                        "\n(redacted stderr saved to {})",
                                        path.display()
                                    )),
                                    Err(err) => {
                                        log::warn!("failed to save stderr safeguard dump: {}", err)
                                    }
                                }
                            }
                            let outcome = apply_git_revert_mode(
                                repo_root,
                                git_revert_mode,
                                log_label,
                                revert_prompt.as_ref(),
                            )?;
                            match outcome {
                                RevertOutcome::Continue { message } => {
                                    if let Some(capture) = timeout_stdout_capture.as_ref()
                                        && let Ok(mut buf) = capture.lock()
                                    {
                                        buf.clear();
                                    }
                                    result = continue_or_rerun(
                                        backend,
                                        &runner_kind,
                                        repo_root,
                                        bins,
                                        &model,
                                        reasoning_effort,
                                        runner_cli,
                                        &message,
                                        &message,
                                        timeout,
                                        permission_mode,
                                        effective_output_handler.clone(),
                                        output_stream,
                                        phase_type,
                                        invocation_session_id.as_deref(),
                                        error_session_id.as_deref(),
                                    );
                                    continue;
                                }
                                RevertOutcome::Reverted {
                                    source: RevertSource::User,
                                } => {
                                    let message =
                                        format_revert_failure_message(terminated_msg, outcome);
                                    return Err(anyhow::Error::new(RunAbort::new(
                                        RunAbortReason::UserRevert,
                                        format!("{}{}", message, safeguard_msg),
                                    )));
                                }
                                _ => {
                                    let message =
                                        format_revert_failure_message(terminated_msg, outcome);
                                    bail!("{}{}", message, safeguard_msg);
                                }
                            }
                        }
                        bail!("{}{}", terminated_msg, safeguard_msg);
                    }
                    Err(err) => {
                        let base_msg = other_msg(err);
                        let message = if revert_on_error {
                            let outcome = apply_git_revert_mode(
                                repo_root,
                                git_revert_mode,
                                log_label,
                                revert_prompt.as_ref(),
                            )?;
                            if matches!(
                                outcome,
                                RevertOutcome::Reverted {
                                    source: RevertSource::User
                                }
                            ) {
                                let message = format_revert_failure_message(&base_msg, outcome);
                                return Err(anyhow::Error::new(RunAbort::new(
                                    RunAbortReason::UserRevert,
                                    message,
                                )));
                            }
                            format_revert_failure_message(&base_msg, outcome)
                        } else {
                            base_msg
                        };
                        bail!("{message}");
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
