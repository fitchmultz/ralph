//! CI failure display and truncation tests.
//!
//! Purpose:
//! - CI failure display and truncation tests.
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

use super::*;

#[test]
fn truncate_for_log_shows_end_of_string() {
    let long = "a".repeat(3000);
    let truncated = truncate_for_log(&long, 100);
    assert!(truncated.starts_with("..."));
    // Should have exactly 103 characters: "..." + 100 'a's
    assert_eq!(truncated.len(), 103);
}

#[test]
fn truncate_for_log_returns_full_if_short() {
    let short = "hello world";
    let truncated = truncate_for_log(short, 100);
    assert_eq!(truncated, short);
}

#[test]
fn truncate_for_log_handles_multibyte_utf8() {
    // Test with multi-byte UTF-8 characters (emoji = 4 bytes each)
    let long = "😀".repeat(100); // 100 emoji = 400 bytes
    let truncated = truncate_for_log(&long, 10); // Keep last 10 chars

    // Should not panic and should produce valid UTF-8
    assert!(truncated.starts_with("..."));
    // After "...", should have exactly 10 emoji characters
    let emoji_part = &truncated[3..]; // Skip "..."
    assert_eq!(emoji_part.chars().count(), 10);
}

#[test]
fn truncate_for_log_handles_empty_string() {
    let truncated = truncate_for_log("", 100);
    assert_eq!(truncated, "");
}

// ========================================================================
// CiFailure Tests
// ========================================================================

#[test]
fn ci_failure_display_includes_exit_code() {
    let failure = CiFailure {
        exit_code: Some(1),
        stdout: String::new(),
        stderr: String::new(),
        error_pattern: None,
    };

    let msg = failure.to_string();
    assert!(
        msg.contains("exit code 1"),
        "Expected 'exit code 1', got: {msg}"
    );
}

#[test]
fn ci_failure_display_includes_error_pattern() {
    let failure = CiFailure {
        exit_code: Some(1),
        stdout: String::new(),
        stderr: String::new(),
        error_pattern: Some("TOML parse error"),
    };

    let msg = failure.to_string();
    assert!(
        msg.contains("[TOML parse error]"),
        "Expected pattern, got: {msg}"
    );
}

#[test]
fn ci_failure_display_includes_truncated_output() {
    let failure = CiFailure {
        exit_code: Some(1),
        stdout: "test output".to_string(),
        stderr: "TOML parse error at line 44".to_string(),
        error_pattern: Some("TOML parse error"),
    };

    let msg = failure.to_string();
    assert!(
        msg.contains(">>> stderr:"),
        "Expected stderr section, got: {msg}"
    );
    assert!(
        msg.contains(">>> stdout:"),
        "Expected stdout section, got: {msg}"
    );
    assert!(
        msg.contains("TOML parse error at line 44"),
        "Expected error message, got: {msg}"
    );
}

#[test]
fn ci_failure_truncates_long_output() {
    let long_output = "x".repeat(1000);
    let failure = CiFailure {
        exit_code: Some(1),
        stdout: long_output.clone(),
        stderr: String::new(),
        error_pattern: None,
    };

    let msg = failure.to_string();
    // Should be truncated, not full 1000 chars
    assert!(
        msg.len() < 800,
        "Message should be truncated, got length {}",
        msg.len()
    );
    assert!(
        msg.contains("..."),
        "Expected truncation marker, got: {msg}"
    );
}

#[test]
fn ci_failure_handles_missing_exit_code() {
    let failure = CiFailure {
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        error_pattern: None,
    };

    let msg = failure.to_string();
    assert!(
        msg.contains("exit code -1"),
        "Expected -1 for missing exit code, got: {msg}"
    );
}

// ========================================================================
// Pattern Detection Tests
// ========================================================================
