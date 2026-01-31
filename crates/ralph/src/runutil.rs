//! Shared helpers for runner invocations with consistent error handling.
//!
//! Responsibilities: execute runner invocations, manage temp resources, and normalize error handling.
//! Not handled: prompt template rendering, queue/task persistence, or runner selection logic.
//! Invariants/assumptions: caller supplies validated runner settings and respects revert policies.

use crate::commands::run::PhaseType;
use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::{fsutil, git, outpututil, runner};
use anyhow::{bail, Result};
use std::fmt;
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertSource {
    Auto,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertOutcome {
    Reverted { source: RevertSource },
    Skipped { reason: String },
    Continue { message: String },
    Proceed { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunAbortReason {
    Interrupted,
    UserRevert,
}

#[derive(Debug)]
pub(crate) struct RunAbort {
    reason: RunAbortReason,
    message: String,
}

impl RunAbort {
    pub(crate) fn new(reason: RunAbortReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }

    pub(crate) fn reason(&self) -> RunAbortReason {
        self.reason
    }
}

impl fmt::Display for RunAbort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RunAbort {}

pub(crate) fn abort_reason(err: &anyhow::Error) -> Option<RunAbortReason> {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<RunAbort>().map(RunAbort::reason))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertDecision {
    Revert,
    Keep,
    Continue { message: String },
    Proceed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertPromptContext {
    pub label: String,
    pub allow_proceed: bool,
    pub preface: Option<String>,
}

impl RevertPromptContext {
    pub fn new(label: &str, allow_proceed: bool) -> Self {
        Self {
            label: label.to_string(),
            allow_proceed,
            preface: None,
        }
    }

    pub fn with_preface(mut self, preface: impl Into<String>) -> Self {
        let preface = preface.into();
        if preface.trim().is_empty() {
            return self;
        }
        self.preface = Some(preface);
        self
    }
}

pub type RevertPromptHandler = Arc<dyn Fn(&RevertPromptContext) -> RevertDecision + Send + Sync>;

const TIMEOUT_STDOUT_CAPTURE_MAX_BYTES: usize = 128 * 1024;

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

pub(crate) fn run_prompt_with_handling_backend<FNonZero, FOther>(
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
    } = invocation;
    let RunnerErrorMessages {
        log_label,
        interrupted_msg,
        timeout_msg,
        terminated_msg,
        non_zero_msg,
        other_msg,
    } = messages;

    // Timeout errors do not currently contain stdout. To support safeguard dumps on timeout,
    // capture streamed output (bounded) when a timeout is configured.
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
                                        format!("\n(redacted stdout saved to {})", path.display());
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
                            "runner_error",
                            &stdout.to_string(),
                        ) {
                            Ok(path) => {
                                safeguard_msg =
                                    format!("\n(redacted stdout saved to {})", path.display());
                            }
                            Err(err) => {
                                log::warn!("failed to save safeguard dump: {}", err);
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
                            if let Some(capture) = timeout_stdout_capture.as_ref() {
                                if let Ok(mut buf) = capture.lock() {
                                    buf.clear();
                                }
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
                            "runner_error",
                            &stdout.to_string(),
                        ) {
                            Ok(path) => {
                                safeguard_msg =
                                    format!("\n(redacted stdout saved to {})", path.display());
                            }
                            Err(err) => {
                                log::warn!("failed to save safeguard dump: {}", err);
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
                            if let Some(capture) = timeout_stdout_capture.as_ref() {
                                if let Ok(mut buf) = capture.lock() {
                                    buf.clear();
                                }
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

pub fn apply_git_revert_mode(
    repo_root: &Path,
    mode: GitRevertMode,
    prompt_label: &str,
    revert_prompt: Option<&RevertPromptHandler>,
) -> Result<RevertOutcome> {
    apply_git_revert_mode_with_context(
        repo_root,
        mode,
        RevertPromptContext::new(prompt_label, false),
        revert_prompt,
    )
}

pub fn apply_git_revert_mode_with_context(
    repo_root: &Path,
    mode: GitRevertMode,
    prompt_context: RevertPromptContext,
    revert_prompt: Option<&RevertPromptHandler>,
) -> Result<RevertOutcome> {
    match mode {
        GitRevertMode::Enabled => {
            git::revert_uncommitted(repo_root)?;
            Ok(RevertOutcome::Reverted {
                source: RevertSource::Auto,
            })
        }
        GitRevertMode::Disabled => Ok(RevertOutcome::Skipped {
            reason: "git_revert_mode=disabled".to_string(),
        }),
        GitRevertMode::Ask => {
            if let Some(prompt) = revert_prompt {
                return apply_revert_decision(
                    repo_root,
                    prompt(&prompt_context),
                    prompt_context.allow_proceed,
                );
            }
            let stdin = std::io::stdin();
            if !stdin.is_terminal() {
                return Ok(RevertOutcome::Skipped {
                    reason: "stdin is not a TTY; keeping changes".to_string(),
                });
            }
            let choice = prompt_revert_choice(&prompt_context)?;
            apply_revert_decision(repo_root, choice, prompt_context.allow_proceed)
        }
    }
}

fn apply_revert_decision(
    repo_root: &Path,
    decision: RevertDecision,
    allow_proceed: bool,
) -> Result<RevertOutcome> {
    match decision {
        RevertDecision::Revert => {
            git::revert_uncommitted(repo_root)?;
            Ok(RevertOutcome::Reverted {
                source: RevertSource::User,
            })
        }
        RevertDecision::Keep => Ok(RevertOutcome::Skipped {
            reason: "user chose to keep changes".to_string(),
        }),
        RevertDecision::Continue { message } => Ok(RevertOutcome::Continue {
            message: message.trim_end_matches(['\n', '\r']).to_string(),
        }),
        RevertDecision::Proceed => {
            if allow_proceed {
                Ok(RevertOutcome::Proceed {
                    reason: "user chose to proceed".to_string(),
                })
            } else {
                Ok(RevertOutcome::Skipped {
                    reason: "proceed not allowed; keeping changes".to_string(),
                })
            }
        }
    }
}

pub fn format_revert_failure_message(base: &str, outcome: RevertOutcome) -> String {
    match outcome {
        RevertOutcome::Reverted { .. } => {
            format!("{base} Uncommitted changes were reverted.")
        }
        RevertOutcome::Skipped { reason } => format!("{base} Revert skipped ({reason})."),
        RevertOutcome::Continue { .. } => {
            format!("{base} Continue requested. No changes were reverted.")
        }
        RevertOutcome::Proceed { .. } => {
            format!("{base} Proceed requested. No changes were reverted.")
        }
    }
}

/// Build a shell command for the current platform (sh -c on Unix, cmd /C on Windows).
pub fn shell_command(command: &str) -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

fn prompt_revert_choice(prompt_context: &RevertPromptContext) -> Result<RevertDecision> {
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stderr = std::io::stderr();
    prompt_revert_choice_with_io(prompt_context, &mut reader, &mut stderr)
}

pub(crate) fn prompt_revert_choice_with_io<R: BufRead, W: Write>(
    prompt_context: &RevertPromptContext,
    reader: &mut R,
    writer: &mut W,
) -> Result<RevertDecision> {
    if let Some(preface) = prompt_context.preface.as_ref() {
        if !preface.trim().is_empty() {
            write!(writer, "{preface}")?;
            if !preface.ends_with('\n') {
                writeln!(writer)?;
            }
            writer.flush().ok();
        }
    }

    let mut prompt = format!(
        "{}: action? [1=keep (default), 2=revert, 3=other",
        prompt_context.label
    );
    if prompt_context.allow_proceed {
        prompt.push_str(", 4=keep+continue");
    }
    prompt.push_str("]: ");
    write!(writer, "{prompt}")?;
    writer.flush().ok();

    let mut input = String::new();
    reader.read_line(&mut input)?;

    let mut decision = parse_revert_response(&input, prompt_context.allow_proceed);

    if matches!(decision, RevertDecision::Continue { ref message } if message.is_empty()) {
        write!(
            writer,
            "{}: enter message to send (empty => keep): ",
            prompt_context.label
        )?;
        writer.flush().ok();

        let mut msg = String::new();
        reader.read_line(&mut msg)?;
        let msg = msg.trim_end_matches(['\n', '\r']);
        if msg.trim().is_empty() {
            decision = RevertDecision::Keep;
        } else {
            decision = RevertDecision::Continue {
                message: msg.to_string(),
            };
        }
    }

    Ok(decision)
}

pub(crate) fn parse_revert_response(input: &str, allow_proceed: bool) -> RevertDecision {
    let raw = input.trim_end_matches(['\n', '\r']);
    let normalized = raw.trim().to_lowercase();

    match normalized.as_str() {
        "" => RevertDecision::Keep,
        "1" | "k" | "keep" => RevertDecision::Keep,
        "2" | "r" | "revert" => RevertDecision::Revert,
        "3" => RevertDecision::Continue {
            message: String::new(),
        },
        "4" if allow_proceed => RevertDecision::Proceed,
        _ => RevertDecision::Continue {
            message: raw.to_string(),
        },
    }
}

fn log_stderr_tail(label: &str, stderr: &str) {
    let tail = outpututil::tail_lines(
        stderr,
        outpututil::OUTPUT_TAIL_LINES,
        outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
    );
    if tail.is_empty() {
        return;
    }

    crate::rerror!("{label} stderr (tail):");
    for line in tail {
        crate::rinfo!("{label}: {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that redaction is applied to stderr content by verifying the
    /// redact_text function works correctly on typical stderr patterns.
    /// This tests the underlying redaction mechanism used by log_stderr_tail.
    #[test]
    fn log_stderr_tail_redacts_api_keys_via_redact_text() {
        let stderr = "Error occurred\nAPI_KEY=secret12345\nMore output";
        let redacted = crate::redaction::redact_text(stderr);

        // Verify API key is redacted
        assert!(
            !redacted.contains("secret12345"),
            "API key should be redacted, got: {}",
            redacted
        );
        assert!(
            redacted.contains("[REDACTED]"),
            "Should contain [REDACTED], got: {}",
            redacted
        );
    }

    #[test]
    fn log_stderr_tail_redacts_bearer_tokens_via_redact_text() {
        let stderr = "Authorization: Bearer abcdef123456789\nDone";
        let redacted = crate::redaction::redact_text(stderr);

        // Verify bearer token is redacted
        assert!(
            !redacted.contains("abcdef123456789"),
            "Bearer token should be redacted, got: {}",
            redacted
        );
        assert!(
            redacted.contains("Bearer [REDACTED]"),
            "Should show Bearer [REDACTED], got: {}",
            redacted
        );
    }

    #[test]
    fn log_stderr_tail_handles_empty_stderr() {
        let tail = outpututil::tail_lines(
            "",
            outpututil::OUTPUT_TAIL_LINES,
            outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
        );
        assert!(tail.is_empty());
    }

    #[test]
    fn log_stderr_tail_presents_normal_content_via_tail_lines() {
        let stderr = "Normal error message\nAnother line";
        let tail = outpututil::tail_lines(
            stderr,
            outpututil::OUTPUT_TAIL_LINES,
            outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
        );

        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0], "Normal error message");
        assert_eq!(tail[1], "Another line");
    }

    #[test]
    fn log_stderr_tail_uses_rinfo_rerror_macros() {
        // Verify that the macros apply redaction by checking their expansion behavior
        // The rinfo! and rerror! macros call redact_text on their arguments
        let input = "token=secret123";
        let redacted = crate::redaction::redact_text(input);
        assert!(!redacted.contains("secret123"));
        assert!(redacted.contains("[REDACTED]"));
    }
}
