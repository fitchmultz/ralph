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

    #[error("runner terminated by signal (signal={signal:?})\nstdout: {stdout}\nstderr: {stderr}")]
    TerminatedBySignal {
        signal: Option<i32>,
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

    // Tests for looks_like_rate_limit
    #[test]
    fn looks_like_rate_limit_detects_429() {
        assert!(looks_like_rate_limit("Error 429"));
        assert!(looks_like_rate_limit("HTTP 429"));
        assert!(!looks_like_rate_limit("Error 500"));
    }

    #[test]
    fn looks_like_rate_limit_detects_variations() {
        assert!(looks_like_rate_limit("rate limit exceeded"));
        assert!(looks_like_rate_limit("Rate Limit Exceeded"));
        assert!(looks_like_rate_limit("too many requests"));
        assert!(looks_like_rate_limit("Too Many Requests"));
        assert!(looks_like_rate_limit("quota exceeded"));
        assert!(looks_like_rate_limit("API throttled"));
    }

    #[test]
    fn looks_like_rate_limit_negative_cases() {
        assert!(!looks_like_rate_limit("success"));
        assert!(!looks_like_rate_limit("internal server error"));
        assert!(!looks_like_rate_limit(""));
    }

    // Tests for looks_like_temporary_unavailable
    #[test]
    fn looks_like_temporary_unavailable_detects_503() {
        assert!(looks_like_temporary_unavailable("Error 503"));
        assert!(looks_like_temporary_unavailable("HTTP 503"));
    }

    #[test]
    fn looks_like_temporary_unavailable_detects_gateway_errors() {
        assert!(looks_like_temporary_unavailable("502 Bad Gateway"));
        assert!(looks_like_temporary_unavailable("504 Gateway Timeout"));
    }

    #[test]
    fn looks_like_temporary_unavailable_detects_variations() {
        assert!(looks_like_temporary_unavailable("service unavailable"));
        assert!(looks_like_temporary_unavailable("Service Unavailable"));
        assert!(looks_like_temporary_unavailable("temporarily unavailable"));
        assert!(looks_like_temporary_unavailable("gateway timeout"));
    }

    #[test]
    fn looks_like_temporary_unavailable_negative_cases() {
        assert!(!looks_like_temporary_unavailable("success"));
        assert!(!looks_like_temporary_unavailable("Error 404"));
        assert!(!looks_like_temporary_unavailable(""));
    }

    // Tests for looks_like_auth_required
    #[test]
    fn looks_like_auth_required_detects_401() {
        let runner = Runner::Gemini;
        assert!(looks_like_auth_required(&runner, "Error 401"));
        assert!(looks_like_auth_required(&runner, "HTTP 401"));
    }

    #[test]
    fn looks_like_auth_required_detects_variations() {
        let runner = Runner::Gemini;
        assert!(looks_like_auth_required(&runner, "unauthorized"));
        assert!(looks_like_auth_required(&runner, "Unauthorized"));
        assert!(looks_like_auth_required(&runner, "invalid api key"));
        assert!(looks_like_auth_required(&runner, "not logged in"));
        assert!(looks_like_auth_required(&runner, "authentication failed"));
        assert!(looks_like_auth_required(&runner, "access denied"));
    }

    #[test]
    fn looks_like_auth_required_negative_cases() {
        let runner = Runner::Gemini;
        assert!(!looks_like_auth_required(&runner, "success"));
        assert!(!looks_like_auth_required(&runner, "Error 500"));
        assert!(!looks_like_auth_required(&runner, ""));
    }

    // Tests for classify() method - NonZeroExit
    #[test]
    fn classify_returns_retryable_for_rate_limit() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "rate limit exceeded".into(),
            stderr: "".into(),
            session_id: None,
        };
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::Retryable(RetryableReason::RateLimited) => {}
            other => panic!("Expected RateLimited, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_retryable_for_503() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "".into(),
            stderr: "HTTP 503 Service Unavailable".into(),
            session_id: None,
        };
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::Retryable(RetryableReason::TemporaryUnavailable) => {}
            other => panic!("Expected TemporaryUnavailable, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_requires_user_input_for_auth() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "401 Unauthorized".into(),
            stderr: "".into(),
            session_id: None,
        };
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::RequiresUserInput(UserInputReason::Auth) => {}
            other => panic!("Expected Auth, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_non_retryable_for_fatal_exit() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "some random error".into(),
            stderr: "no matching pattern".into(),
            session_id: None,
        };
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit) => {}
            other => panic!("Expected FatalExit, got {:?}", other),
        }
    }

    // Tests for classify() method - Other Error Variants
    #[test]
    fn classify_binary_missing_requires_user_input() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = RunnerError::BinaryMissing {
            bin: "test".to_string(),
            source: io_err,
        };
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::RequiresUserInput(UserInputReason::MissingBinary) => {}
            other => panic!("Expected MissingBinary, got {:?}", other),
        }
    }

    #[test]
    fn classify_timeout_is_retryable() {
        let err = RunnerError::Timeout;
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::Retryable(RetryableReason::TemporaryUnavailable) => {}
            other => panic!("Expected TemporaryUnavailable, got {:?}", other),
        }
    }

    #[test]
    fn classify_interrupted_is_non_retryable() {
        let err = RunnerError::Interrupted;
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit) => {}
            other => panic!("Expected FatalExit, got {:?}", other),
        }
    }

    #[test]
    fn classify_io_transient_errors_are_retryable() {
        use std::io::ErrorKind;

        let transient_kinds = [
            ErrorKind::TimedOut,
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted,
            ErrorKind::ConnectionRefused,
            ErrorKind::NotConnected,
            ErrorKind::UnexpectedEof,
            ErrorKind::WouldBlock,
        ];

        for kind in &transient_kinds {
            let io_err = std::io::Error::new(*kind, "transient error");
            let err = RunnerError::Io(io_err);
            let runner = Runner::Gemini;
            match err.classify(&runner) {
                RunnerFailureClass::Retryable(RetryableReason::TransientIo) => {}
                other => panic!("Expected TransientIo for {:?}, got {:?}", kind, other),
            }
        }
    }

    #[test]
    fn classify_io_other_errors_are_non_retryable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
        let err = RunnerError::Io(io_err);
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit) => {}
            other => panic!("Expected FatalExit, got {:?}", other),
        }
    }

    #[test]
    fn classify_other_error_with_rate_limit_pattern() {
        let err = RunnerError::Other(anyhow!("429 rate limit exceeded"));
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::Retryable(RetryableReason::RateLimited) => {}
            other => panic!("Expected RateLimited, got {:?}", other),
        }
    }

    #[test]
    fn classify_other_error_with_auth_pattern() {
        let err = RunnerError::Other(anyhow!("401 invalid api key"));
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::RequiresUserInput(UserInputReason::Auth) => {}
            other => panic!("Expected Auth, got {:?}", other),
        }
    }

    #[test]
    fn classify_other_error_without_pattern_is_non_retryable() {
        let err = RunnerError::Other(anyhow!("some generic error"));
        let runner = Runner::Gemini;
        match err.classify(&runner) {
            RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit) => {}
            other => panic!("Expected FatalExit, got {:?}", other),
        }
    }
}
