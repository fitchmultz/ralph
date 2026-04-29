//! Continue-session escalation tests for CI retries.
//!
//! Purpose:
//! - Continue-session escalation tests for CI retries.
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

fn continue_session_for_ci_tests() -> crate::commands::run::supervision::ContinueSession {
    crate::commands::run::supervision::ContinueSession {
        runner: crate::contracts::Runner::Codex,
        model: crate::contracts::Model::Gpt53Codex,
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: Some("sess-123".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        run_event_handler: None,
        ci_failure_retry_count: CI_GATE_AUTO_RETRY_LIMIT,
        task_id: "RQ-0947".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    }
}

#[test]
fn run_ci_gate_with_continue_session_escalates_on_threshold_same_pattern() -> Result<()> {
    let temp = TempDir::new()?;
    let command = "python3 -c \"import sys; print('ruff failed: TOML parse error at line 44', file=sys.stderr); raise SystemExit(1)\"";

    write_repo_trust(temp.path());
    let resolved = resolved_with_ci_command(temp.path(), Some(command.to_string()), true);
    let mut session = continue_session_for_ci_tests();
    session.ci_failure_retry_count = 0;
    session.last_ci_error_pattern = Some("TOML parse error".to_string());
    session.consecutive_same_error_count = CI_FAILURE_ESCALATION_THRESHOLD - 1;

    let err = run_ci_gate_with_continue_session(
        &resolved,
        crate::contracts::GitRevertMode::Disabled,
        None,
        &mut session,
        |_output, _elapsed| -> Result<()> { panic!("on_resume should not be called") },
        None,
    )
    .expect_err("expected escalation on repeated identical CI error");

    let msg = err.to_string();
    assert!(msg.contains("MANUAL INTERVENTION REQUIRED"));
    assert!(msg.contains("same error"));
    assert!(msg.contains("TOML parse error"));
    assert_eq!(
        session.consecutive_same_error_count,
        CI_FAILURE_ESCALATION_THRESHOLD
    );
    Ok(())
}

#[test]
fn run_ci_gate_with_continue_session_escalation_honors_continue_choice() -> Result<()> {
    let temp = TempDir::new()?;
    let command = "python3 -c \"import sys; print('format-check failed', file=sys.stderr); raise SystemExit(1)\"";

    write_repo_trust(temp.path());
    let resolved = resolved_with_ci_command(temp.path(), Some(command.to_string()), true);
    let mut resolved = resolved;
    resolved.config.agent.codex_bin = Some(
        temp.path()
            .join("missing-codex")
            .to_string_lossy()
            .to_string(),
    );
    let mut session = continue_session_for_ci_tests();
    session.session_id = None;
    session.ci_failure_retry_count = 0;
    session.last_ci_error_pattern = Some("Format check failure".to_string());
    session.consecutive_same_error_count = CI_FAILURE_ESCALATION_THRESHOLD - 1;

    let prompt_handler: crate::runutil::RevertPromptHandler = Arc::new(|context| {
        assert_eq!(context.label, "CI failure escalation");
        Ok(crate::runutil::RevertDecision::Continue {
            message: "Run the formatter and fix the test failure.".to_string(),
        })
    });

    let err = run_ci_gate_with_continue_session(
        &resolved,
        crate::contracts::GitRevertMode::Ask,
        Some(&prompt_handler),
        &mut session,
        |_output, _elapsed| -> Result<()> { panic!("on_resume should not be called") },
        None,
    )
    .expect_err("expected continue path to attempt fresh invocation and fail on missing runner");

    let msg = err.to_string();
    assert!(msg.contains("runner binary not found"));
    assert!(
        !msg.contains("MANUAL INTERVENTION REQUIRED"),
        "escalation continue path should attempt resume instead of immediate manual bailout"
    );
    Ok(())
}

#[test]
fn run_ci_gate_with_continue_session_resets_counter_when_pattern_changes() -> Result<()> {
    let temp = TempDir::new()?;
    let command = "python3 -c \"import sys; print('format-check failed', file=sys.stderr); raise SystemExit(1)\"";

    write_repo_trust(temp.path());
    let resolved = resolved_with_ci_command(temp.path(), Some(command.to_string()), true);
    let mut session = continue_session_for_ci_tests();
    session.ci_failure_retry_count = CI_GATE_AUTO_RETRY_LIMIT;
    session.last_ci_error_pattern = Some("TOML parse error".to_string());
    session.consecutive_same_error_count = CI_FAILURE_ESCALATION_THRESHOLD - 1;

    let _ = run_ci_gate_with_continue_session(
        &resolved,
        crate::contracts::GitRevertMode::Disabled,
        None,
        &mut session,
        |_output, _elapsed| -> Result<()> { panic!("on_resume should not be called") },
        None,
    )
    .expect_err("expected CI failure after counter reset path");

    assert_eq!(session.consecutive_same_error_count, 1);
    assert_eq!(
        session.last_ci_error_pattern.as_deref(),
        Some("Format check failure")
    );
    Ok(())
}

#[test]
fn run_ci_gate_with_continue_session_clears_pattern_tracking_after_success() -> Result<()> {
    let temp = TempDir::new()?;
    let command = "python3 -c \"raise SystemExit(0)\"";

    write_repo_trust(temp.path());
    let resolved = resolved_with_ci_command(temp.path(), Some(command.to_string()), true);
    let mut session = continue_session_for_ci_tests();
    session.last_ci_error_pattern = Some("TOML parse error".to_string());
    session.consecutive_same_error_count = 2;

    run_ci_gate_with_continue_session(
        &resolved,
        crate::contracts::GitRevertMode::Disabled,
        None,
        &mut session,
        |_output, _elapsed| -> Result<()> { Ok(()) },
        None,
    )?;

    assert_eq!(session.last_ci_error_pattern, None);
    assert_eq!(session.consecutive_same_error_count, 0);
    Ok(())
}
