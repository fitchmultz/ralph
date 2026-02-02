//! Runner execution helpers with consistent error handling.
//!
//! Responsibilities:
//! - Execute runner invocations via `RunnerBackend`.
//! - Normalize error handling across runner errors (timeouts, interrupts, signals, non-zero exits).
//! - Apply git revert policies and produce consistent user-facing messages.
//!
//! Not handled here:
//! - Prompt template rendering.
//! - Queue/task persistence.
//! - Runner binary resolution (callers supply `RunnerBinaries`).
//!
//! Invariants/assumptions:
//! - Callers provide validated runner/model settings.
//! - When `revert_on_error` is true, revert behavior is delegated to `revert` submodule.
//! - Timeout safeguard capture is bounded to avoid unbounded memory growth.

use anyhow::{Result, bail};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::commands::run::PhaseType;
use crate::constants::buffers::TIMEOUT_STDOUT_CAPTURE_MAX_BYTES;
use crate::constants::buffers::{OUTPUT_TAIL_LINE_MAX_CHARS, OUTPUT_TAIL_LINES};
use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::{fsutil, outpututil, runner};

use super::abort::{RunAbort, RunAbortReason};
use super::revert::{
    RevertOutcome, RevertPromptHandler, RevertSource, apply_git_revert_mode,
    format_revert_failure_message,
};

pub(crate) struct RunnerInvocation<'a> {
    pub repo_root: &'a Path,
    pub runner_kind: Runner,
    pub bins: runner::RunnerBinaries<'a>,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: runner::ResolvedRunnerCliOptions,
    pub prompt: &'a str,
    pub timeout: Option<Duration>,
    pub permission_mode: Option<ClaudePermissionMode>,
    /// If true, revert uncommitted changes on runner errors.
    /// Set to false for task to preserve user's existing work.
    pub revert_on_error: bool,
    /// Policy for reverting uncommitted changes when errors occur.
    pub git_revert_mode: GitRevertMode,
    /// Optional callback for streaming runner output.
    pub output_handler: Option<runner::OutputHandler>,
    /// Controls whether runner output is streamed to stdout/stderr.
    pub output_stream: runner::OutputStream,
    /// Optional handler for revert prompts (interactive UIs).
    pub revert_prompt: Option<RevertPromptHandler>,
    /// The type of phase being executed (for runner-specific behavior).
    pub phase_type: PhaseType,
    /// Optional session ID for runners that support session resumption (e.g., Kimi).
    /// When provided, the runner will use this ID for the session.
    pub session_id: Option<String>,
}

pub struct RunnerErrorMessages<'a, FNonZero, FOther>
where
    FNonZero: FnOnce(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    pub log_label: &'a str,
    pub interrupted_msg: &'a str,
    pub timeout_msg: &'a str,
    pub terminated_msg: &'a str,
    pub non_zero_msg: FNonZero,
    pub other_msg: FOther,
}

pub trait RunnerBackend {
    #[allow(clippy::too_many_arguments)]
    fn run_prompt<'a>(
        &mut self,
        runner_kind: Runner,
        work_dir: &Path,
        bins: runner::RunnerBinaries<'a>,
        model: Model,
        reasoning_effort: Option<ReasoningEffort>,
        runner_cli: runner::ResolvedRunnerCliOptions,
        prompt: &str,
        timeout: Option<Duration>,
        permission_mode: Option<ClaudePermissionMode>,
        output_handler: Option<runner::OutputHandler>,
        output_stream: runner::OutputStream,
        phase_type: PhaseType,
        session_id: Option<String>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError>;

    #[allow(clippy::too_many_arguments)]
    fn resume_session<'a>(
        &mut self,
        runner_kind: Runner,
        work_dir: &Path,
        bins: runner::RunnerBinaries<'a>,
        model: Model,
        reasoning_effort: Option<ReasoningEffort>,
        runner_cli: runner::ResolvedRunnerCliOptions,
        session_id: &str,
        message: &str,
        permission_mode: Option<ClaudePermissionMode>,
        timeout: Option<Duration>,
        output_handler: Option<runner::OutputHandler>,
        output_stream: runner::OutputStream,
        phase_type: PhaseType,
    ) -> Result<runner::RunnerOutput, runner::RunnerError>;
}

struct RealRunnerBackend;

impl RunnerBackend for RealRunnerBackend {
    fn run_prompt<'a>(
        &mut self,
        runner_kind: Runner,
        work_dir: &Path,
        bins: runner::RunnerBinaries<'a>,
        model: Model,
        reasoning_effort: Option<ReasoningEffort>,
        runner_cli: runner::ResolvedRunnerCliOptions,
        prompt: &str,
        timeout: Option<Duration>,
        permission_mode: Option<ClaudePermissionMode>,
        output_handler: Option<runner::OutputHandler>,
        output_stream: runner::OutputStream,
        phase_type: PhaseType,
        session_id: Option<String>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        runner::run_prompt(
            runner_kind,
            work_dir,
            bins,
            model,
            reasoning_effort,
            runner_cli,
            prompt,
            timeout,
            permission_mode,
            output_handler,
            output_stream,
            phase_type,
            session_id,
        )
    }

    fn resume_session<'a>(
        &mut self,
        runner_kind: Runner,
        work_dir: &Path,
        bins: runner::RunnerBinaries<'a>,
        model: Model,
        reasoning_effort: Option<ReasoningEffort>,
        runner_cli: runner::ResolvedRunnerCliOptions,
        session_id: &str,
        message: &str,
        permission_mode: Option<ClaudePermissionMode>,
        timeout: Option<Duration>,
        output_handler: Option<runner::OutputHandler>,
        output_stream: runner::OutputStream,
        phase_type: PhaseType,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        runner::resume_session(
            runner_kind,
            work_dir,
            bins,
            model,
            reasoning_effort,
            runner_cli,
            session_id,
            message,
            permission_mode,
            timeout,
            output_handler,
            output_stream,
            phase_type,
        )
    }
}

fn wrap_output_handler_with_capture(
    existing: Option<runner::OutputHandler>,
    max_bytes: usize,
) -> (Arc<Mutex<String>>, Option<runner::OutputHandler>) {
    let capture = Arc::new(Mutex::new(String::new()));
    let capture_for_handler = capture.clone();
    let existing_for_handler = existing.clone();

    let handler: runner::OutputHandler = Arc::new(Box::new(move |chunk: &str| {
        if let Ok(mut buf) = capture_for_handler.lock() {
            buf.push_str(chunk);
            if buf.len() > max_bytes {
                let excess = buf.len() - max_bytes;
                buf.drain(..excess);
            }
        }
        if let Some(existing) = existing_for_handler.as_ref() {
            (existing)(chunk);
        }
    }));

    (capture, Some(handler))
}

pub fn run_prompt_with_handling_backend<FNonZero, FOther>(
    invocation: RunnerInvocation<'_>,
    messages: RunnerErrorMessages<'_, FNonZero, FOther>,
    backend: &mut impl RunnerBackend,
) -> Result<runner::RunnerOutput>
where
    FNonZero: FnOnce(i32) -> String,
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
        session_id,
    } = invocation;
    let RunnerErrorMessages {
        log_label,
        interrupted_msg,
        timeout_msg,
        terminated_msg,
        non_zero_msg,
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

    let mut result = backend.run_prompt(
        runner_kind,
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
        session_id.clone(),
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
            Err(runner::RunnerError::Timeout) => {
                let mut safeguard_msg = String::new();
                let message = if revert_on_error {
                    if let Some(capture) = timeout_stdout_capture.as_ref() {
                        let captured = capture.lock().map(|buf| buf.clone()).unwrap_or_default();
                        if !captured.trim().is_empty() {
                            match fsutil::safeguard_text_dump_redacted("runner_error", &captured) {
                                Ok(path) => {
                                    safeguard_msg =
                                        format!("\n(redacted output saved to {})", path.display());
                                }
                                Err(err) => {
                                    log::warn!("failed to save safeguard dump: {}", err);
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
                            source: RevertSource::User,
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
                session_id,
            }) => {
                log_stderr_tail(log_label, &stderr.to_string());
                let mut safeguard_msg = String::new();
                if revert_on_error {
                    if !stdout.0.is_empty() {
                        match fsutil::safeguard_text_dump_redacted(
                            "runner_error_stdout",
                            &stdout.to_string(),
                        ) {
                            Ok(path) => {
                                safeguard_msg =
                                    format!("\n(redacted stdout saved to {})", path.display());
                            }
                            Err(err) => {
                                log::warn!("failed to save stdout safeguard dump: {}", err);
                            }
                        }
                    }
                    if !stderr.0.is_empty() {
                        match fsutil::safeguard_text_dump_redacted(
                            "runner_error_stderr",
                            &stderr.to_string(),
                        ) {
                            Ok(path) => {
                                safeguard_msg.push_str(&format!(
                                    "\n(redacted stderr saved to {})",
                                    path.display()
                                ));
                            }
                            Err(err) => {
                                log::warn!("failed to save stderr safeguard dump: {}", err);
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
                            let Some(session_id) = session_id.as_deref() else {
                                bail!("Catastrophic: no session id captured; cannot Continue.");
                            };
                            if let Some(capture) = timeout_stdout_capture.as_ref()
                                && let Ok(mut buf) = capture.lock()
                            {
                                buf.clear();
                            }
                            result = backend.resume_session(
                                runner_kind,
                                repo_root,
                                bins,
                                model.clone(),
                                reasoning_effort,
                                runner_cli,
                                session_id,
                                &message,
                                permission_mode,
                                timeout,
                                effective_output_handler.clone(),
                                output_stream,
                                phase_type,
                            );
                            continue;
                        }
                        RevertOutcome::Reverted {
                            source: RevertSource::User,
                        } => {
                            let message =
                                format_revert_failure_message(&non_zero_msg(code), outcome);
                            return Err(anyhow::Error::new(RunAbort::new(
                                RunAbortReason::UserRevert,
                                format!("{}{}", message, safeguard_msg),
                            )));
                        }
                        _ => {
                            let message =
                                format_revert_failure_message(&non_zero_msg(code), outcome);
                            bail!("{}{}", message, safeguard_msg);
                        }
                    }
                }
                bail!("{}{}", non_zero_msg(code), safeguard_msg);
            }
            Err(runner::RunnerError::TerminatedBySignal {
                stdout,
                stderr,
                session_id,
            }) => {
                log_stderr_tail(log_label, &stderr.to_string());
                let mut safeguard_msg = String::new();
                if revert_on_error {
                    if !stdout.0.is_empty() {
                        match fsutil::safeguard_text_dump_redacted(
                            "runner_error_stdout",
                            &stdout.to_string(),
                        ) {
                            Ok(path) => {
                                safeguard_msg =
                                    format!("\n(redacted stdout saved to {})", path.display());
                            }
                            Err(err) => {
                                log::warn!("failed to save stdout safeguard dump: {}", err);
                            }
                        }
                    }
                    if !stderr.0.is_empty() {
                        match fsutil::safeguard_text_dump_redacted(
                            "runner_error_stderr",
                            &stderr.to_string(),
                        ) {
                            Ok(path) => {
                                safeguard_msg.push_str(&format!(
                                    "\n(redacted stderr saved to {})",
                                    path.display()
                                ));
                            }
                            Err(err) => {
                                log::warn!("failed to save stderr safeguard dump: {}", err);
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
                            let Some(session_id) = session_id.as_deref() else {
                                bail!("Catastrophic: no session id captured; cannot Continue.");
                            };
                            if let Some(capture) = timeout_stdout_capture.as_ref()
                                && let Ok(mut buf) = capture.lock()
                            {
                                buf.clear();
                            }
                            result = backend.resume_session(
                                runner_kind,
                                repo_root,
                                bins,
                                model.clone(),
                                reasoning_effort,
                                runner_cli,
                                session_id,
                                &message,
                                permission_mode,
                                timeout,
                                effective_output_handler.clone(),
                                output_stream,
                                phase_type,
                            );
                            continue;
                        }
                        RevertOutcome::Reverted {
                            source: RevertSource::User,
                        } => {
                            let message = format_revert_failure_message(terminated_msg, outcome);
                            return Err(anyhow::Error::new(RunAbort::new(
                                RunAbortReason::UserRevert,
                                format!("{}{}", message, safeguard_msg),
                            )));
                        }
                        _ => {
                            let message = format_revert_failure_message(terminated_msg, outcome);
                            bail!("{}{}", message, safeguard_msg);
                        }
                    }
                }
                bail!("{}{}", terminated_msg, safeguard_msg);
            }
            Err(err) => {
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
                            source: RevertSource::User,
                        }
                    ) {
                        let message = format_revert_failure_message(&other_msg(err), outcome);
                        return Err(anyhow::Error::new(RunAbort::new(
                            RunAbortReason::UserRevert,
                            message,
                        )));
                    }
                    format_revert_failure_message(&other_msg(err), outcome)
                } else {
                    other_msg(err)
                };
                bail!("{message}");
            }
        }
    }
}

pub(crate) fn run_prompt_with_handling<FNonZero, FOther>(
    invocation: RunnerInvocation<'_>,
    messages: RunnerErrorMessages<'_, FNonZero, FOther>,
) -> Result<runner::RunnerOutput>
where
    FNonZero: FnOnce(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    let mut backend = RealRunnerBackend;
    run_prompt_with_handling_backend(invocation, messages, &mut backend)
}

fn log_stderr_tail(label: &str, stderr: &str) {
    let tail = outpututil::tail_lines(stderr, OUTPUT_TAIL_LINES, OUTPUT_TAIL_LINE_MAX_CHARS);
    if tail.is_empty() {
        return;
    }

    crate::rerror!("{label} stderr (tail):");
    for line in tail {
        crate::rinfo!("{label}: {line}");
    }
}
