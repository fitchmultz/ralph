//! Runner error surface and contextual constructors.
//!
//! Responsibilities:
//! - Define `RunnerError`, the matchable error type for runner orchestration.
//! - Provide helpers to construct contextual `RunnerError::Other` values.
//! - Classify failures as retryable vs non-retryable vs requires-user-input.
//!
//! Does not handle:
//! - Runner/model validation (see `runner/model.rs`).
//! - Command assembly and process execution (see `runner/execution/*`).
//!
//! Assumptions/invariants:
//! - Any user-visible stdout/stderr stored in errors must be redacted via `RedactedString`
//!   (or redacted at display time by downstream formatting).

use std::fmt;

use anyhow::anyhow;

use crate::contracts::Runner;
use crate::redaction::RedactedString;

/// Classification of runner failures for retry decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunnerFailureClass {
    /// Transient failure; safe to automatically retry.
    Retryable(RetryableReason),
    /// User action required; should not be retried.
    RequiresUserInput(UserInputReason),
    /// Deterministic failure; do not retry.
    NonRetryable(NonRetryableReason),
}

/// Reasons why a failure is considered retryable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryableReason {
    /// Rate limit or quota exceeded (HTTP 429, etc).
    RateLimited,
    /// Temporary service unavailability (HTTP 503, etc).
    TemporaryUnavailable,
    /// Transient I/O error (connection reset, timeout, etc).
    TransientIo,
}

/// Reasons why a failure requires user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UserInputReason {
    /// Authentication required (API key, login, etc).
    Auth,
    /// Required binary is missing.
    MissingBinary,
    /// Setup required (configuration, installation, etc).
    #[allow(dead_code)]
    SetupRequired,
}

/// Reasons why a failure is considered non-retryable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NonRetryableReason {
    /// Invalid invocation or bad arguments.
    InvalidInvocation,
    /// Fatal exit; no point in retrying.
    FatalExit,
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("runner binary not found: {bin}")]
    BinaryMissing {
        bin: String,
        #[source]
        source: std::io::Error,
    },

    #[error("runner failed to spawn: {bin}")]
    SpawnFailed {
        bin: String,
        #[source]
        source: std::io::Error,
    },

    #[error("runner exited non-zero (code={code})\nstdout: {stdout}\nstderr: {stderr}")]
    NonZeroExit {
        code: i32,
        stdout: RedactedString,
        stderr: RedactedString,
        session_id: Option<String>,
    },

    #[error("runner terminated by signal\nstdout: {stdout}\nstderr: {stderr}")]
    TerminatedBySignal {
        stdout: RedactedString,
        stderr: RedactedString,
        session_id: Option<String>,
    },

    #[error("runner interrupted")]
    Interrupted,

    #[error("runner timed out")]
    Timeout,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("other error: {0}")]
    Other(#[from] anyhow::Error),
}

fn runner_label(runner: &Runner) -> String {
    match runner {
        Runner::Codex => "codex".to_string(),
        Runner::Opencode => "opencode".to_string(),
        Runner::Gemini => "gemini".to_string(),
        Runner::Cursor => "cursor".to_string(),
        Runner::Claude => "claude".to_string(),
        Runner::Kimi => "kimi".to_string(),
        Runner::Pi => "pi".to_string(),
        Runner::Plugin(id) => format!("plugin:{}", id),
    }
}

/// Check if text looks like a rate limit error.
fn looks_like_rate_limit(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("quota exceeded")
        || lower.contains("throttled")
}

/// Check if text looks like a temporary unavailability error.
fn looks_like_temporary_unavailable(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("503")
        || lower.contains("service unavailable")
        || lower.contains("temporarily unavailable")
        || lower.contains("gateway timeout")
        || lower.contains("502")
        || lower.contains("504")
}

/// Check if text looks like an auth error.
fn looks_like_auth_required(_runner: &Runner, text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("not logged in")
        || lower.contains("authentication failed")
        || lower.contains("access denied")
}

/// Classify textual failure based on exit code and output.
fn classify_textual_failure(
    runner: &Runner,
    _code: i32,
    stdout: &str,
    stderr: &str,
) -> RunnerFailureClass {
    let combined = format!("{} {}", stdout, stderr);
    let text = combined.to_lowercase();

    if looks_like_rate_limit(&text) {
        return RunnerFailureClass::Retryable(RetryableReason::RateLimited);
    }
    if looks_like_temporary_unavailable(&text) {
        return RunnerFailureClass::Retryable(RetryableReason::TemporaryUnavailable);
    }
    if looks_like_auth_required(runner, &text) {
        return RunnerFailureClass::RequiresUserInput(UserInputReason::Auth);
    }

    RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit)
}

impl RunnerError {
    /// Classify this error for retry decisions.
    ///
    /// Conservative policy: only clearly transient cases are classified as retryable.
    pub(crate) fn classify(&self, runner: &Runner) -> RunnerFailureClass {
        match self {
            RunnerError::BinaryMissing { .. } => {
                RunnerFailureClass::RequiresUserInput(UserInputReason::MissingBinary)
            }
            RunnerError::SpawnFailed { .. } => {
                // Usually deterministic; keep non-retryable for now.
                RunnerFailureClass::NonRetryable(NonRetryableReason::InvalidInvocation)
            }
            RunnerError::Interrupted => {
                RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit)
            }
            RunnerError::Timeout => {
                // Conservative: treat as retryable only if caller opts in via config.
                RunnerFailureClass::Retryable(RetryableReason::TemporaryUnavailable)
            }
            RunnerError::Io(e) => {
                use std::io::ErrorKind;
                match e.kind() {
                    ErrorKind::TimedOut
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::NotConnected
                    | ErrorKind::UnexpectedEof
                    | ErrorKind::WouldBlock => {
                        RunnerFailureClass::Retryable(RetryableReason::TransientIo)
                    }
                    _ => RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit),
                }
            }
            RunnerError::NonZeroExit {
                code,
                stdout,
                stderr,
                ..
            } => classify_textual_failure(runner, *code, &stdout.to_string(), &stderr.to_string()),
            RunnerError::TerminatedBySignal { .. } => {
                // Usually not safe to retry automatically.
                RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit)
            }
            RunnerError::Other(err) => {
                let msg = format!("{:#}", err).to_lowercase();
                if looks_like_rate_limit(&msg) {
                    RunnerFailureClass::Retryable(RetryableReason::RateLimited)
                } else if looks_like_temporary_unavailable(&msg) {
                    RunnerFailureClass::Retryable(RetryableReason::TemporaryUnavailable)
                } else if looks_like_auth_required(runner, &msg) {
                    RunnerFailureClass::RequiresUserInput(UserInputReason::Auth)
                } else {
                    RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit)
                }
            }
        }
    }
}

pub(crate) fn runner_execution_error(runner: &Runner, bin: &str, step: &str) -> RunnerError {
    RunnerError::Other(anyhow!(
        "Runner execution failed (runner={}, bin={}): {}.",
        runner_label(runner),
        bin,
        step
    ))
}

pub(crate) fn runner_execution_error_with_source(
    runner: &Runner,
    bin: &str,
    step: &str,
    source: impl fmt::Display,
) -> RunnerError {
    RunnerError::Other(anyhow!(
        "Runner execution failed (runner={}, bin={}): {}: {}.",
        runner_label(runner),
        bin,
        step,
        source
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_error_nonzero_exit_redacts_output() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "out: API_KEY=secret123".into(),
            stderr: "err: bearer abc123def456".into(),
            session_id: None,
        };
        let msg = format!("{err}");
        assert!(msg.contains("API_KEY=[REDACTED]"));
        assert!(msg.contains("bearer [REDACTED]"));
        assert!(!msg.contains("secret123"));
        assert!(!msg.contains("abc123def456"));
    }

    #[test]
    fn runner_execution_error_includes_context() {
        let err = runner_execution_error(&Runner::Gemini, "gemini", "capture child stdout");
        let msg = format!("{err}");
        assert!(msg.contains("runner=gemini"));
        assert!(msg.contains("bin=gemini"));
        assert!(msg.contains("capture child stdout"));
    }
}
