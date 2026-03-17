//! Purpose: Unit-test hub for `crate::runutil` submodules.
//!
//! Responsibilities:
//! - Validate stderr tail redaction assumptions used by execution logging.
//! - Keep broader runutil regression hubs wired to focused companion modules.
//!
//! Scope:
//! - Root-level runutil smoke tests only.
//! - Detailed runner orchestration regressions live in `execution/orchestration/tests.rs`.
//!
//! Usage:
//! - Compiled through `runutil.rs` under `#[cfg(test)]`.
//!
//! Invariants/Assumptions:
//! - Tests may use mock backends and temp dirs through companion test modules.
//! - Real runner binaries are not required for this root test hub.

use crate::constants::buffers::{OUTPUT_TAIL_LINE_MAX_CHARS, OUTPUT_TAIL_LINES};

#[path = "tests/fixtures.rs"]
mod fixtures;
#[path = "tests/revert.rs"]
mod revert;
#[path = "tests/runner_handling.rs"]
mod runner_handling;
#[path = "tests/validation.rs"]
mod validation;

#[test]
fn log_stderr_tail_redacts_api_keys_via_redact_text() {
    let stderr = "Error occurred\nAPI_KEY=secret12345\nMore output";
    let redacted = crate::redaction::redact_text(stderr);

    assert!(
        !redacted.contains("secret12345"),
        "API key should be redacted"
    );
    assert!(redacted.contains("[REDACTED]"), "Should contain [REDACTED]");
}

#[test]
fn log_stderr_tail_redacts_bearer_tokens_via_redact_text() {
    let stderr = "Authorization: Bearer abcdef123456789\nDone";
    let redacted = crate::redaction::redact_text(stderr);

    assert!(
        !redacted.contains("abcdef123456789"),
        "Bearer token should be redacted"
    );
    assert!(
        redacted.contains("Bearer [REDACTED]"),
        "Should show Bearer [REDACTED]"
    );
}

#[test]
fn log_stderr_tail_handles_empty_stderr() {
    let tail = crate::outpututil::tail_lines("", OUTPUT_TAIL_LINES, OUTPUT_TAIL_LINE_MAX_CHARS);
    assert!(tail.is_empty());
}

#[test]
fn log_stderr_tail_presents_normal_content_via_tail_lines() {
    let stderr = "Normal error message\nAnother line";
    let tail = crate::outpututil::tail_lines(stderr, OUTPUT_TAIL_LINES, OUTPUT_TAIL_LINE_MAX_CHARS);

    assert_eq!(tail.len(), 2);
    assert_eq!(tail[0], "Normal error message");
    assert_eq!(tail[1], "Another line");
}

#[test]
fn log_stderr_tail_uses_rinfo_rerror_macros() {
    let input = "token=secret123";
    let redacted = crate::redaction::redact_text(input);
    assert!(!redacted.contains("secret123"));
    assert!(redacted.contains("[REDACTED]"));
}

#[test]
fn safeguard_dump_redacts_secrets_in_stderr() {
    use crate::redaction::RedactedString;

    let stderr_content = "Error: API_KEY=sk-abc123xyz789\nAuthorization: Bearer secret_token";
    let stdout = RedactedString::from("stdout content");
    let stderr = RedactedString::from(stderr_content);

    let stdout_str = stdout.to_string();
    let stderr_str = stderr.to_string();

    assert!(
        !stderr_str.contains("sk-abc123xyz789"),
        "API key should be redacted in stderr: {}",
        stderr_str
    );
    assert!(
        !stderr_str.contains("secret_token"),
        "Bearer token should be redacted in stderr: {}",
        stderr_str
    );
    assert!(
        stderr_str.contains("[REDACTED]"),
        "Redacted marker should be present: {}",
        stderr_str
    );

    assert!(
        stdout_str.contains("stdout content"),
        "Normal stdout should be preserved: {}",
        stdout_str
    );
}
