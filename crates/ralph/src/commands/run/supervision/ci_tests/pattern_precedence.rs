//! CI pattern precedence and edge-case tests.
//!
//! Purpose:
//! - CI pattern precedence and edge-case tests.
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
fn detect_ci_error_pattern_cases() {
    struct Case {
        stdout: &'static str,
        stderr: &'static str,
        want: Option<&'static str>,
        want_line: Option<u32>,
        want_invalid: Option<&'static str>,
    }

    let cases = [
        Case {
            stdout: "",
            stderr: "ruff failed: TOML parse error at line 44, column 18",
            want: Some("TOML parse error"),
            want_line: Some(44),
            want_invalid: None,
        },
        Case {
            stdout: "",
            stderr: "unknown variant `py314`, expected one of py37, py313",
            want: Some("Unknown variant error"),
            want_line: None,
            want_invalid: Some("py314"),
        },
        Case {
            stdout: "",
            stderr: "TOML parse error at line 10: unknown variant `foo`",
            want: Some("TOML parse error"),
            want_line: Some(10),
            want_invalid: Some("foo"),
        },
        Case {
            stdout: "TOML parse error",
            stderr: "",
            want: Some("TOML parse error"),
            want_line: None,
            want_invalid: None,
        },
        Case {
            stdout: "",
            stderr: "ruff: error checking configuration",
            want: Some("Ruff error"),
            want_line: None,
            want_invalid: None,
        },
        Case {
            stdout: "",
            stderr: "format-check failed: 3 files need formatting",
            want: Some("Format check failure"),
            want_line: None,
            want_invalid: None,
        },
        Case {
            stdout: "",
            stderr: "lint check failed with 5 errors",
            want: Some("Lint check failure"),
            want_line: None,
            want_invalid: None,
        },
        Case {
            stdout: "all good",
            stderr: "",
            want: None,
            want_line: None,
            want_invalid: None,
        },
        Case {
            stdout: "build succeeded",
            stderr: "test passed",
            want: None,
            want_line: None,
            want_invalid: None,
        },
        Case {
            stdout: "",
            stderr: "error: something went wrong",
            want: None,
            want_line: None,
            want_invalid: None,
        },
        Case {
            stdout: "",
            stderr: "pyproject.toml:100:5: error",
            want: None,
            want_line: None,
            want_invalid: None,
        },
    ];

    for case in cases {
        let got = detect_ci_error_pattern(case.stdout, case.stderr);
        assert_eq!(
            got.as_ref().map(|p| p.pattern_type),
            case.want,
            "stderr={} stdout={}",
            case.stderr,
            case.stdout
        );
        if let Some(pattern) = got {
            assert_eq!(
                pattern.line_number, case.want_line,
                "line_number mismatch for stderr={} stdout={}",
                case.stderr, case.stdout
            );
            assert_eq!(
                pattern.invalid_value.as_deref(),
                case.want_invalid,
                "invalid_value mismatch for stderr={} stdout={}",
                case.stderr,
                case.stdout
            );
        }
    }
}

#[test]
fn detect_toml_takes_precedence_over_unknown_variant() {
    let output = "TOML parse error at line 44: unknown variant `py314`";
    let pattern = detect_ci_error_pattern("", output).unwrap();
    assert_eq!(pattern.pattern_type, "TOML parse error");
    assert_eq!(pattern.line_number, Some(44));
}

#[test]
fn detect_toml_takes_precedence_over_ruff() {
    let output = "ruff failed: TOML parse error at line 50";
    let pattern = detect_ci_error_pattern("", output).unwrap();
    assert_eq!(pattern.pattern_type, "TOML parse error");
    assert_eq!(pattern.line_number, Some(50));
}

#[test]
fn detect_unknown_variant_takes_precedence_over_ruff() {
    let output = "ruff: unknown variant `bad`";
    let pattern = detect_ci_error_pattern("", output).unwrap();
    assert_eq!(pattern.pattern_type, "Unknown variant error");
}

#[test]
fn detect_format_takes_precedence_over_lint_when_both_present() {
    let pattern = detect_ci_error_pattern(
        "format-check failed: 1 file needs formatting",
        "lint check failed with 2 errors",
    )
    .unwrap();
    assert_eq!(pattern.pattern_type, "Format check failure");
}

#[test]
fn extract_valid_values_handles_period_terminator() {
    let output = "expected one of foo, bar, baz.";
    assert_eq!(
        extract_valid_values(output),
        Some("foo, bar, baz".to_string())
    );
}

#[test]
fn extract_valid_values_handles_newline_terminator() {
    let output = "expected one of a, b\nc";
    assert_eq!(extract_valid_values(output), Some("a, b".to_string()));
}

#[test]
fn extract_line_number_handles_comma_suffix() {
    let output = "at line 42, column 10";
    assert_eq!(extract_line_number(output), Some(42));
}

#[test]
fn detect_format_case_insensitive() {
    let output = "FORMAT-CHECK FAILED";
    let pattern = detect_format_check_error(output).unwrap();
    assert_eq!(pattern.pattern_type, "Format check failure");
}

#[test]
fn detect_lint_case_insensitive() {
    let output = "LINT CHECK FAILED";
    let pattern = detect_lint_check_error(output).unwrap();
    assert_eq!(pattern.pattern_type, "Lint check failure");
}

#[test]
fn detect_ruff_yields_to_toml_parse() {
    let output = "ruff failed: TOML parse error";
    let pattern = detect_ruff_error(output);
    assert!(
        pattern.is_none(),
        "ruff detector should yield to TOML parse"
    );
}

// ========================================================================
// CI Escalation Threshold Tests
// ========================================================================
