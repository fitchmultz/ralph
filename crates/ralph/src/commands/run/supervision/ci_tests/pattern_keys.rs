//! CI pattern keying and aggregate-case tests.
//!
//! Purpose:
//! - CI pattern keying and aggregate-case tests.
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
fn get_error_pattern_key_returns_pattern_type() {
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "TOML parse error at line 44".to_string(),
    };
    assert_eq!(
        get_error_pattern_key(&result),
        Some("TOML parse error".to_string())
    );
}

#[test]
fn get_error_pattern_key_returns_none_for_unrecognized() {
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: "some random output".to_string(),
        stderr: String::new(),
    };
    assert_eq!(get_error_pattern_key(&result), None);
}

#[test]
fn get_error_pattern_key_detects_unknown_variant() {
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "unknown variant `py314`, expected one of py37, py38".to_string(),
    };
    assert_eq!(
        get_error_pattern_key(&result),
        Some("Unknown variant error".to_string())
    );
}

#[test]
fn get_error_pattern_key_detects_format_check() {
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "format-check failed".to_string(),
    };
    assert_eq!(
        get_error_pattern_key(&result),
        Some("Format check failure".to_string())
    );
}

#[test]
fn get_error_pattern_key_detects_lint_check() {
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "lint check failed".to_string(),
    };
    assert_eq!(
        get_error_pattern_key(&result),
        Some("Lint check failure".to_string())
    );
}

#[test]
fn get_error_pattern_key_combines_stdout_stderr() {
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: "some output".to_string(),
        stderr: "TOML parse error at line 10".to_string(),
    };
    assert_eq!(
        get_error_pattern_key(&result),
        Some("TOML parse error".to_string())
    );
}

// ========================================================================
// Table-Driven Pattern Detection Tests
// ========================================================================
