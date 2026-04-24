//! CI pattern helper and compliance-message tests.
//!
//! Purpose:
//! - CI pattern helper and compliance-message tests.
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
fn detect_toml_parse_error_extracts_line_number() {
    let output = "ruff failed: TOML parse error at line 44, column 18: unknown variant `py314`";
    let pattern = detect_toml_parse_error(output).unwrap();
    assert_eq!(pattern.line_number, Some(44));
    assert_eq!(pattern.pattern_type, "TOML parse error");
}

#[test]
fn detect_toml_parse_error_returns_none_for_non_toml() {
    let output = "Some random error message";
    assert!(detect_toml_parse_error(output).is_none());
}

#[test]
fn detect_unknown_variant_extracts_values() {
    let output =
        "unknown variant `py314`, expected one of py37, py38, py39, py310, py311, py312, py313";
    let pattern = detect_unknown_variant_error(output).unwrap();
    assert_eq!(pattern.invalid_value, Some("py314".to_string()));
    assert!(pattern.valid_values.unwrap().contains("py313"));
    assert_eq!(pattern.pattern_type, "Unknown variant error");
}

#[test]
fn detect_unknown_variant_returns_none_for_non_variant() {
    let output = "Some error without variant";
    assert!(detect_unknown_variant_error(output).is_none());
}

#[test]
fn detect_ruff_error_returns_pattern() {
    let output = "ruff failed with some error";
    let pattern = detect_ruff_error(output).unwrap();
    assert_eq!(pattern.pattern_type, "Ruff error");
    assert_eq!(pattern.file_path, Some("pyproject.toml".to_string()));
}

#[test]
fn detect_format_check_error_returns_pattern() {
    let output = "format-check failed";
    let pattern = detect_format_check_error(output).unwrap();
    assert_eq!(pattern.pattern_type, "Format check failure");
}

#[test]
fn detect_lint_check_error_returns_pattern() {
    let output = "lint check failed";
    let pattern = detect_lint_check_error(output).unwrap();
    assert_eq!(pattern.pattern_type, "Lint check failure");
}

#[test]
fn detect_lock_contention_error_returns_pattern() {
    let output = "Blocking waiting for file lock on build directory";
    let pattern = detect_lock_contention_error(output).unwrap();
    assert_eq!(pattern.pattern_type, "Lock contention");
}

#[test]
fn detect_ci_error_pattern_combines_stdout_stderr() {
    let stdout = "Some output";
    let stderr = "TOML parse error at line 10";
    let pattern = detect_ci_error_pattern(stdout, stderr).unwrap();
    assert_eq!(pattern.line_number, Some(10));
}

#[test]
fn detect_ci_error_pattern_returns_none_on_clean_output() {
    let output = "All tests passed!";
    assert!(detect_ci_error_pattern(output, "").is_none());
}

#[test]
fn compliance_message_includes_lock_contention_guidance() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "Blocking waiting for file lock on build directory".to_string(),
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);
    assert!(msg.contains("Lock contention"));
    assert!(msg.contains("waiting on a file lock"));
}

#[test]
fn extract_line_number_from_at_line_pattern() {
    let output = "Error at line 42";
    assert_eq!(extract_line_number(output), Some(42));
}

#[test]
fn extract_line_number_from_colon_pattern() {
    let output = "pyproject.toml:44:18: error";
    assert_eq!(extract_line_number(output), Some(44));
}

#[test]
fn extract_line_number_returns_none_when_not_present() {
    let output = "No line number here";
    assert!(extract_line_number(output).is_none());
}

#[test]
fn extract_invalid_value_finds_backtick_value() {
    let output = "unknown variant `py314`, expected...";
    assert_eq!(extract_invalid_value(output), Some("py314".to_string()));
}

#[test]
fn extract_invalid_value_handles_unicode_prefix() {
    let output = "İstanbul: unknown variant `py314`, expected one of py37, py313";
    assert_eq!(extract_invalid_value(output), Some("py314".to_string()));
}

#[test]
fn extract_valid_values_finds_expected_list() {
    let output = "expected one of py37, py38, py313";
    assert_eq!(
        extract_valid_values(output),
        Some("py37, py38, py313".to_string())
    );
}

#[test]
fn extract_valid_values_handles_unicode_prefix() {
    let output = "İstanbul: expected one of py37, py313.";
    assert_eq!(
        extract_valid_values(output),
        Some("py37, py313".to_string())
    );
}

#[test]
fn infer_file_path_extracts_explicit_toml_file() {
    let output = "ruff failed parsing pyproject.toml, unknown variant `py314`";
    assert_eq!(infer_file_path(output), Some("pyproject.toml".to_string()));
}

#[test]
fn infer_file_path_infers_pyproject_from_ruff_parse_context() {
    let output = "ruff failed: parse error at line 5";
    assert_eq!(infer_file_path(output), Some("pyproject.toml".to_string()));
}

#[test]
fn compliance_message_includes_detected_toml_error() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "TOML parse error at line 44: unknown variant `py314`, expected one of py37, py313"
            .to_string(),
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);
    assert!(
        msg.contains("DETECTED ERROR"),
        "Should contain DETECTED ERROR section"
    );
    assert!(
        msg.contains("TOML parse error"),
        "Should identify error type"
    );
    assert!(msg.contains("**Line**"), "Should show Line label");
    assert!(msg.contains("44"), "Should show line 44");
    assert!(msg.contains("py314"), "Should show invalid value");
}

#[test]
fn compliance_message_includes_detected_unknown_variant() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "unknown variant `foo`, expected one of bar, baz".to_string(),
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);
    assert!(
        msg.contains("DETECTED ERROR"),
        "Should contain DETECTED ERROR section"
    );
    assert!(
        msg.contains("Unknown variant error"),
        "Should identify error type"
    );
    assert!(msg.contains("`foo`"), "Should show invalid value");
    assert!(msg.contains("bar, baz"), "Should show valid options");
}

#[test]
fn compliance_message_no_detected_section_on_clean_output() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: "build failed".to_string(),
        stderr: String::new(),
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);
    assert!(
        !msg.contains("DETECTED ERROR"),
        "Should NOT contain DETECTED ERROR section for unrecognized errors"
    );
    // Should still have common patterns
    assert!(msg.contains("COMMON PATTERNS"));
}

#[test]
fn format_detected_pattern_includes_all_fields() {
    let pattern = DetectedErrorPattern {
        pattern_type: "Test error",
        file_path: Some("test.toml".to_string()),
        line_number: Some(10),
        invalid_value: Some("bad_value".to_string()),
        valid_values: Some("good1, good2".to_string()),
        guidance: "Fix the error",
    };

    let formatted = format_detected_pattern(&pattern);
    assert!(formatted.contains("Test error"));
    assert!(formatted.contains("test.toml"));
    assert!(formatted.contains("10"));
    assert!(formatted.contains("bad_value"));
    assert!(formatted.contains("good1, good2"));
    assert!(formatted.contains("Fix the error"));
}

// ========================================================================
// Error Pattern Key Tests
// ========================================================================
