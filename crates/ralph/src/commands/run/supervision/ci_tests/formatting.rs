//! CI gate execution and output-formatting tests.
//!
//! Purpose:
//! - CI gate execution and output-formatting tests.
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
fn ci_gate_command_label_returns_default() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    assert_eq!(ci_gate_command_label(&resolved), "make ci");
}

#[test]
fn ci_gate_command_label_returns_custom() {
    let temp = TempDir::new().unwrap();
    let mut resolved = resolved_with_ci_command(temp.path(), None, true);
    resolved.config.agent.ci_gate = Some(CiGateConfig {
        enabled: Some(true),
        argv: Some(vec!["cargo".to_string(), "test".to_string()]),
    });
    assert_eq!(ci_gate_command_label(&resolved), "cargo test");
}

#[test]
fn run_ci_gate_skips_when_disabled() -> Result<()> {
    let temp = TempDir::new()?;
    let resolved = resolved_with_ci_command(temp.path(), Some("make ci".to_string()), false);
    // Should succeed without running anything, returning success
    let result = run_ci_gate(&resolved)?;
    assert!(result.success);
    Ok(())
}

#[test]
fn run_ci_gate_errors_on_empty_command() {
    let temp = TempDir::new().unwrap();
    write_repo_trust(temp.path());
    let mut resolved = resolved_with_ci_command(temp.path(), None, true);
    resolved.config.agent.ci_gate = Some(CiGateConfig {
        enabled: Some(true),
        argv: Some(vec!["".to_string()]),
    });
    let err = run_ci_gate(&resolved).unwrap_err();
    assert!(format!("{err:#}").contains("CI gate argv entries must be non-empty"));
}

#[test]
fn run_ci_gate_captures_output() -> Result<()> {
    let temp = TempDir::new()?;
    let command = "python3 -c \"import sys; print('stdout text'); print('stderr text', file=sys.stderr); raise SystemExit(1)\"";
    write_repo_trust(temp.path());
    let resolved = resolved_with_ci_command(temp.path(), Some(command.to_string()), true);
    let err = run_ci_gate(&resolved).unwrap_err();

    // CI failure now returns Err(CiFailure)
    let ci_failure = err.downcast::<CiFailure>().unwrap();
    assert_eq!(ci_failure.exit_code, Some(1));
    assert!(ci_failure.stdout.contains("stdout text"));
    assert!(ci_failure.stderr.contains("stderr text"));
    Ok(())
}

#[test]
fn format_ci_output_includes_stderr_first() {
    let stdout = "line1\nline2\nline3";
    let stderr = "error1\nerror2";
    let result = format_ci_output_for_message(stdout, stderr, 50, 50);

    // stderr should appear in output
    assert!(result.contains("error1"));
    assert!(result.contains("error2"));
}

#[test]
fn format_ci_output_shows_head_and_tail() {
    let stdout = (1..=200)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let stderr = "";

    // Request 50 head + 50 tail
    let result = format_ci_output_for_message(&stdout, stderr, 50, 50);

    // Should show total line count
    assert!(result.contains("200 lines total"));

    // Should show explicit line ranges
    assert!(result.contains("showing lines 1-50 and 151-200"));

    // Should include early lines (format/lint errors appear here)
    assert!(result.contains("line1"));
    assert!(result.contains("line50"));

    // Should include late lines (test failures appear here)
    assert!(result.contains("line151"));
    assert!(result.contains("line200"));

    // Should NOT include middle lines
    assert!(!result.contains("line51"));
    assert!(!result.contains("line100"));
    assert!(!result.contains("line150"));

    // Should indicate truncation
    assert!(result.contains("100 lines omitted"));
}

#[test]
fn format_ci_output_shows_all_when_small() {
    let stdout = "line1\nline2\nline3";
    let stderr = "";

    let result = format_ci_output_for_message(stdout, stderr, 50, 50);

    // Should show all without truncation
    assert!(result.contains("3 lines)"));
    assert!(result.contains("line1"));
    assert!(result.contains("line3"));
    assert!(!result.contains("omitted"));
}

#[test]
fn format_ci_output_handles_empty() {
    let result = format_ci_output_for_message("", "", 50, 50);
    assert!(result.contains("No output captured"));
}

#[test]
fn compliance_message_includes_exit_code_and_output() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(2),
        stdout: "test output".to_string(),
        stderr: "error: ruff failed".to_string(),
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);
    // Should show numeric exit code, not Debug format like "Some(2)"
    assert!(
        msg.contains("exit code 2"),
        "Expected 'exit code 2', got: {msg}"
    );
    assert!(msg.contains("ruff failed"));
}

#[test]
fn compliance_message_includes_formatted_ci_output_with_ranges() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);

    // Create large output that will be truncated
    let stdout = (1..=200)
        .map(|i| format!("out-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let stderr = (1..=10)
        .map(|i| format!("err-{i}"))
        .collect::<Vec<_>>()
        .join("\n");

    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout,
        stderr,
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);

    // Should include formatted output with line ranges
    assert!(
        msg.contains("lines total"),
        "Should show total lines in message"
    );
    assert!(
        msg.contains("showing lines"),
        "Should show explicit line ranges"
    );
    assert!(
        msg.contains("err-1"),
        "Should include early stderr in output"
    );
    assert!(
        msg.contains("out-200"),
        "Should include late stdout in output"
    );
    assert!(
        msg.contains("lines omitted"),
        "Should indicate truncation when output is large"
    );
    assert!(
        msg.contains("Fix the errors above before continuing."),
        "Should include enforcement guidance"
    );
}

#[test]
fn format_ci_output_handles_zero_head_budget() {
    let stdout = (1..=8)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");

    let result = format_ci_output_for_message(&stdout, "", 0, 3);

    assert!(result.contains("8 lines total"));
    assert!(result.contains("showing lines 6-8"));
    assert!(result.contains("line6"));
    assert!(result.contains("line8"));
    assert!(result.contains("5 lines omitted"));
    assert!(!result.contains("1-0"));
}

#[test]
fn format_ci_output_handles_zero_tail_budget() {
    let stdout = (1..=8)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");

    let result = format_ci_output_for_message(&stdout, "", 3, 0);

    assert!(result.contains("8 lines total"));
    assert!(result.contains("showing lines 1-3"));
    assert!(result.contains("line1"));
    assert!(result.contains("line3"));
    assert!(result.contains("5 lines omitted"));
    assert!(!result.contains("9-8"));
}

#[test]
fn format_ci_output_handles_zero_total_budget() {
    let stdout = "line1\nline2\nline3";

    let result = format_ci_output_for_message(stdout, "", 0, 0);

    assert!(result.contains("3 lines total; snippet budget is 0 lines"));
    assert!(result.contains("3 lines omitted"));
    assert!(!result.contains("```"));
}

#[test]
fn compliance_message_orders_output_before_enforcement_text() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(2),
        stdout: "out-1\nout-2".to_string(),
        stderr: "err-1".to_string(),
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);

    let output_idx = msg
        .find("CI output (")
        .expect("message should include CI output snippet");
    let fix_idx = msg
        .find("Fix the errors above before continuing.")
        .expect("message should include enforcement text");

    assert!(
        output_idx < fix_idx,
        "output snippet should appear before enforcement guidance"
    );
}

#[test]
fn build_ci_failure_message_with_user_input_includes_ci_output() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: "test stdout output".to_string(),
        stderr: "ruff failed: TOML parse error".to_string(),
    };
    let user_message = "Please check the pyproject.toml file";

    let combined = build_ci_failure_message_with_user_input(&resolved, &result, user_message);

    // Should include CI output context
    assert!(
        combined.contains("CI output ("),
        "should include CI output header"
    );
    assert!(
        combined.contains("ruff failed: TOML parse error"),
        "should include stderr from CI"
    );
    assert!(combined.contains("exit code 1"), "should include exit code");

    // Should include user message
    assert!(
        combined.contains(user_message),
        "should include user message"
    );

    // Should include enforcement guidance
    assert!(
        combined.contains("Fix the errors above before continuing."),
        "should include enforcement guidance"
    );

    // CI output should come before user message
    let ci_output_idx = combined.find("CI output (").unwrap();
    let user_msg_idx = combined.find(user_message).unwrap();
    assert!(
        ci_output_idx < user_msg_idx,
        "CI output should appear before user message"
    );
}

#[test]
fn build_ci_failure_message_with_empty_user_input_returns_strict_message_only() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(1),
        stdout: "test stdout output".to_string(),
        stderr: "ruff failed: TOML parse error".to_string(),
    };

    let combined = build_ci_failure_message_with_user_input(&resolved, &result, " \n\t ");
    let strict = strict_ci_gate_compliance_message(&resolved, &result);

    assert_eq!(combined, strict);
    assert!(!combined.contains("Agent message from user intervention:"));
}

#[test]
fn compliance_message_includes_troubleshooting_patterns() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(2),
        stdout: String::new(),
        stderr: String::new(),
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);
    assert!(msg.contains("TOML parse error"));
    assert!(msg.contains("unknown variant"));
    assert!(msg.contains("format-check failed"));
    assert!(msg.contains("lint-check failed"));
}

#[test]
fn compliance_message_contains_required_enforcement_language() {
    let temp = TempDir::new().unwrap();
    let resolved = resolved_with_ci_command(temp.path(), None, true);
    let result = CiGateResult {
        success: false,
        exit_code: Some(2),
        stdout: "fmt-check passed".to_string(),
        stderr: "ruff failed: TOML parse error".to_string(),
    };

    let msg = strict_ci_gate_compliance_message(&resolved, &result);
    assert!(
        msg.contains("CI gate (make ci): CI failed with exit code 2"),
        "Expected CI gate prefix with exit code, got: {msg}"
    );
    assert!(
        msg.contains("Fix the errors above before continuing."),
        "Expected remediation instruction, got: {msg}"
    );
    assert!(
        msg.contains("COMMON PATTERNS:"),
        "Expected COMMON PATTERNS section, got: {msg}"
    );
    assert!(
        msg.contains("ruff failed: TOML parse error"),
        "Expected CI output context in message, got: {msg}"
    );
}
