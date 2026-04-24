//! Runner output formatting tests.
//!
//! Purpose:
//! - Runner output formatting tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::runner::{OutputStream, RunnerOutput};
use std::process::ExitStatus;

#[test]
fn runner_output_display_redacts_output() {
    let output = RunnerOutput {
        status: ExitStatus::default(),
        stdout: "out: API_KEY=secret123".to_string(),
        stderr: "err: bearer abc123def456".to_string(),
        session_id: None,
    };
    let msg = format!("{}", output);
    assert!(msg.contains("API_KEY=[REDACTED]"));
    assert!(msg.contains("bearer [REDACTED]"));
    assert!(!msg.contains("secret123"));
    assert!(!msg.contains("abc123def456"));
}

#[test]
fn output_stream_terminal_allows_terminal_output() {
    assert!(OutputStream::Terminal.streams_to_terminal());
}

#[test]
fn output_stream_handler_only_suppresses_terminal_output() {
    assert!(!OutputStream::HandlerOnly.streams_to_terminal());
}
