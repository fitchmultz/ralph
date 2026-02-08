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
use crate::runner::{RetryableReason, RunnerFailureClass};
use crate::{fsutil, outpututil, runner};

use super::abort::{RunAbort, RunAbortReason};
use super::revert::{
    RevertOutcome, RevertPromptHandler, RevertSource, apply_git_revert_mode,
    format_revert_failure_message,
};
use super::{SeededRng, compute_backoff, format_duration};

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
    /// Retry policy for transient failures.
    pub retry_policy: super::RunnerRetryPolicy,
}

pub(crate) struct RunnerErrorMessages<'a, FNonZero, FOther>
where
    FNonZero: FnMut(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    pub log_label: &'a str,
    pub interrupted_msg: &'a str,
    pub timeout_msg: &'a str,
    pub terminated_msg: &'a str,
    pub non_zero_msg: FNonZero,
    pub other_msg: FOther,
}

pub(crate) trait RunnerBackend {
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
        plugins: Option<&crate::plugins::registry::PluginRegistry>,
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
        plugins: Option<&crate::plugins::registry::PluginRegistry>,
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
        plugins: Option<&crate::plugins::registry::PluginRegistry>,
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
            plugins,
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
        plugins: Option<&crate::plugins::registry::PluginRegistry>,
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
            plugins,
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
        fn append_chunk(buf: &mut String, chunk: &str, max_bytes: usize) {
            buf.push_str(chunk);
            if buf.len() > max_bytes {
                let excess = buf.len() - max_bytes;
                buf.drain(..excess);
            }
        }

        match capture_for_handler.lock() {
            Ok(mut buf) => {
                append_chunk(&mut buf, chunk, max_bytes);
            }
            Err(poisoned) => {
                log::warn!("timeout_stdout_capture mutex poisoned; recovering captured output");
                let mut buf = poisoned.into_inner();
                append_chunk(&mut buf, chunk, max_bytes);
            }
        }
        if let Some(existing) = existing_for_handler.as_ref() {
            (existing)(chunk);
        }
    }));

    (capture, Some(handler))
}

/// Emit an operation marker for UI clients/log viewers.
fn emit_operation(handler: &Option<runner::OutputHandler>, msg: &str) {
    if let Some(h) = handler.as_ref() {
        (h)(&format!("RALPH_OPERATION: {}\n", msg));
    }
}

/// Check if we should attempt retry based on repo state.
/// Returns true if repo is clean enough to retry, or if we can auto-revert.
fn should_retry_with_repo_state(
    repo_root: &Path,
    revert_on_error: bool,
    git_revert_mode: GitRevertMode,
) -> Result<bool> {
    // Check if repo is dirty only in allowed paths
    let dirty_only_allowed = match crate::git::clean::repo_dirty_only_allowed_paths(
        repo_root,
        crate::git::clean::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    ) {
        Ok(value) => value,
        Err(err) => {
            // Retry is a best-effort UX improvement. If we cannot reliably determine repo
            // state (e.g. not a git repo), skip retry instead of overriding the runner error.
            log::warn!("Failed to check repo state for retry; skipping retry: {err}");
            return Ok(false);
        }
    };

    if dirty_only_allowed {
        return Ok(true);
    }

    // If we can auto-revert without prompting, retry is allowed
    if revert_on_error && git_revert_mode == GitRevertMode::Enabled {
        return Ok(true);
    }

    Ok(false)
}

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
        session_id,
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

    // Retry state
    let mut attempt: u32 = 1;
    let max_attempts = retry_policy.max_attempts;
    let mut rng = SeededRng::new();

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
        session_id.clone(),
        None,
    );

    loop {
        match result {
            Ok(output) => return Ok(output),
            Err(runner::RunnerError::Interrupted) => {
                // Never retry interruptions
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
                // Classify the error for retry decision
                let classification = err.classify(&runner_kind);

                // Check if we should retry
                if attempt < max_attempts
                    && matches!(classification, RunnerFailureClass::Retryable(_))
                {
                    // Check repo state for safe retry
                    let should_retry =
                        should_retry_with_repo_state(repo_root, revert_on_error, git_revert_mode)?;

                    if should_retry {
                        // Auto-revert if enabled and repo is dirty
                        if revert_on_error
                            && git_revert_mode == GitRevertMode::Enabled
                            && let Err(e) = crate::git::revert_uncommitted(repo_root)
                        {
                            log::warn!("Failed to auto-revert before retry: {}", e);
                        }

                        // Compute backoff
                        let delay = compute_backoff(retry_policy, attempt, &mut rng);
                        let reason_str = match classification {
                            RunnerFailureClass::Retryable(RetryableReason::RateLimited) => {
                                "rate limit"
                            }
                            RunnerFailureClass::Retryable(
                                RetryableReason::TemporaryUnavailable,
                            ) => "temporarily unavailable",
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

                        // Check for Ctrl-C during backoff
                        std::thread::sleep(delay);
                        if let Ok(ctrlc) = runner::ctrlc_state() {
                            use std::sync::atomic::Ordering;
                            if ctrlc.interrupted.load(Ordering::SeqCst) {
                                return Err(anyhow::Error::new(RunAbort::new(
                                    RunAbortReason::Interrupted,
                                    interrupted_msg.to_string(),
                                )));
                            }
                        }

                        attempt += 1;
                        emit_operation(
                            &effective_output_handler,
                            &format!("Running runner attempt {}/ {}", attempt, max_attempts),
                        );

                        // Retry the runner invocation
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
                            session_id.clone(),
                            None,
                        );
                        continue;
                    }
                }

                // Fall through to existing error handling
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
                                        bail!(
                                            "Catastrophic: no session id captured; cannot Continue."
                                        );
                                    };
                                    if let Some(capture) = timeout_stdout_capture.as_ref()
                                        && let Ok(mut buf) = capture.lock()
                                    {
                                        buf.clear();
                                    }
                                    result = backend.resume_session(
                                        runner_kind.clone(),
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
                                        None,
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
                                        safeguard_msg = format!(
                                            "\n(redacted stdout saved to {})",
                                            path.display()
                                        );
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
                                        bail!(
                                            "Catastrophic: no session id captured; cannot Continue."
                                        );
                                    };
                                    if let Some(capture) = timeout_stdout_capture.as_ref()
                                        && let Ok(mut buf) = capture.lock()
                                    {
                                        buf.clear();
                                    }
                                    result = backend.resume_session(
                                        runner_kind.clone(),
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
                                        None,
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
                                    source: RevertSource::User,
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
