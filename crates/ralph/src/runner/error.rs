//! Runner error surface and contextual constructors.
//!
//! Responsibilities:
//! - Define `RunnerError`, the matchable error type for runner orchestration.
//! - Provide helpers to construct contextual `RunnerError::Other` values.
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

fn runner_label(runner: Runner) -> &'static str {
    match runner {
        Runner::Codex => "codex",
        Runner::Opencode => "opencode",
        Runner::Gemini => "gemini",
        Runner::Cursor => "cursor",
        Runner::Claude => "claude",
        Runner::Kimi => "kimi",
        Runner::Pi => "pi",
    }
}

pub(crate) fn runner_execution_error(runner: Runner, bin: &str, step: &str) -> RunnerError {
    RunnerError::Other(anyhow!(
        "Runner execution failed (runner={}, bin={}): {}.",
        runner_label(runner),
        bin,
        step
    ))
}

pub(crate) fn runner_execution_error_with_source(
    runner: Runner,
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
        let err = runner_execution_error(Runner::Gemini, "gemini", "capture child stdout");
        let msg = format!("{err}");
        assert!(msg.contains("runner=gemini"));
        assert!(msg.contains("bin=gemini"));
        assert!(msg.contains("capture child stdout"));
    }
}
