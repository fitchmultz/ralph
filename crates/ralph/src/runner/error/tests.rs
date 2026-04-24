//! Runner error regression tests.
//!
//! Purpose:
//! - Runner error regression tests.
//!
//! Responsibilities:
//! - Verify runner error formatting remains redacted and user-facing context stays intact.
//! - Cover retry classification heuristics for textual, IO, and fatal failures.
//! - Lock down helper constructors used by runner execution paths.
//!
//! Non-scope:
//! - Runner subprocess integration or command assembly.
//! - Model validation or provider-specific invocation flows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Tests exercise the parent `runner::error` module through `super::*`.
//! - Redaction-sensitive strings must never leak into formatted messages.

use anyhow::anyhow;

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
    assert!(!looks_like_auth_required(&runner, "rate limit exceeded"));
    assert!(!looks_like_auth_required(&runner, ""));
}

#[test]
fn classify_returns_retryable_for_rate_limit() {
    let runner = Runner::Gemini;
    let class = classify_textual_failure(&runner, 1, "", "rate limit exceeded");
    assert_eq!(
        class,
        RunnerFailureClass::Retryable(RetryableReason::RateLimited)
    );
}

#[test]
fn classify_returns_retryable_for_503() {
    let runner = Runner::Gemini;
    let class = classify_textual_failure(&runner, 1, "", "service unavailable");
    assert_eq!(
        class,
        RunnerFailureClass::Retryable(RetryableReason::TemporaryUnavailable)
    );
}

#[test]
fn classify_returns_requires_user_input_for_auth() {
    let runner = Runner::Gemini;
    let class = classify_textual_failure(&runner, 1, "", "invalid api key");
    assert_eq!(
        class,
        RunnerFailureClass::RequiresUserInput(UserInputReason::Auth)
    );
}

#[test]
fn classify_returns_non_retryable_for_fatal_exit() {
    let runner = Runner::Gemini;
    let class = classify_textual_failure(&runner, 1, "", "fatal error");
    assert_eq!(
        class,
        RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit)
    );
}

#[test]
fn classify_binary_missing_requires_user_input() {
    let err = RunnerError::BinaryMissing {
        bin: "missing-runner".to_string(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
    };
    assert_eq!(
        err.classify(&Runner::Gemini),
        RunnerFailureClass::RequiresUserInput(UserInputReason::MissingBinary)
    );
}

#[test]
fn classify_timeout_is_retryable() {
    let err = RunnerError::Timeout;
    assert_eq!(
        err.classify(&Runner::Gemini),
        RunnerFailureClass::Retryable(RetryableReason::TemporaryUnavailable)
    );
}

#[test]
fn classify_interrupted_is_non_retryable() {
    let err = RunnerError::Interrupted;
    assert_eq!(
        err.classify(&Runner::Gemini),
        RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit)
    );
}

#[test]
fn classify_io_transient_errors_are_retryable() {
    for kind in [
        std::io::ErrorKind::TimedOut,
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::ConnectionAborted,
        std::io::ErrorKind::ConnectionRefused,
        std::io::ErrorKind::NotConnected,
        std::io::ErrorKind::UnexpectedEof,
        std::io::ErrorKind::WouldBlock,
    ] {
        let err = RunnerError::Io(std::io::Error::new(kind, "transient"));
        assert_eq!(
            err.classify(&Runner::Gemini),
            RunnerFailureClass::Retryable(RetryableReason::TransientIo)
        );
    }
}

#[test]
fn classify_io_other_errors_are_non_retryable() {
    let err = RunnerError::Io(std::io::Error::other("fatal"));
    assert_eq!(
        err.classify(&Runner::Gemini),
        RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit)
    );
}

#[test]
fn classify_other_error_with_rate_limit_pattern() {
    let err = RunnerError::Other(anyhow!("429 too many requests"));
    assert_eq!(
        err.classify(&Runner::Gemini),
        RunnerFailureClass::Retryable(RetryableReason::RateLimited)
    );
}

#[test]
fn classify_other_error_with_auth_pattern() {
    let err = RunnerError::Other(anyhow!("401 unauthorized"));
    assert_eq!(
        err.classify(&Runner::Gemini),
        RunnerFailureClass::RequiresUserInput(UserInputReason::Auth)
    );
}

#[test]
fn classify_other_error_without_pattern_is_non_retryable() {
    let err = RunnerError::Other(anyhow!("fatal configuration error"));
    assert_eq!(
        err.classify(&Runner::Gemini),
        RunnerFailureClass::NonRetryable(NonRetryableReason::FatalExit)
    );
}
