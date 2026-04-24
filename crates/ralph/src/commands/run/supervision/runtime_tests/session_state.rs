//! Continue-session state preservation regressions.
//!
//! Purpose:
//! - Continue-session state preservation regressions.
//!
//! Responsibilities:
//! - Validate `ContinueSession` stores CLI override state and phase type without re-resolution.
//! - Keep lightweight state-only regressions out of orchestration-heavy suites.
//!
//! Not handled here:
//! - Runner subprocess execution.
//! - Queue, git, or CI orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests operate on plain structs without touching the filesystem.

use super::support::continue_session_with;
use crate::commands::run::PhaseType;
use crate::contracts::{
    Runner, RunnerApprovalMode, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode,
    RunnerVerbosity, UnsupportedOptionPolicy,
};

#[test]
fn continue_session_preserves_runner_cli_options() {
    let custom_runner_cli = crate::runner::ResolvedRunnerCliOptions {
        output_format: RunnerOutputFormat::StreamJson,
        verbosity: RunnerVerbosity::Quiet,
        approval_mode: RunnerApprovalMode::Safe,
        sandbox: RunnerSandboxMode::Enabled,
        plan_mode: RunnerPlanMode::Enabled,
        unsupported_option_policy: UnsupportedOptionPolicy::Error,
    };

    let mut session = continue_session_with(
        Runner::Codex,
        Some("test-session"),
        PhaseType::Implementation,
    );
    session.model = crate::contracts::Model::Gpt53Codex;
    session.runner_cli = custom_runner_cli;

    assert_eq!(session.runner_cli.verbosity, RunnerVerbosity::Quiet);
    assert_eq!(session.runner_cli.approval_mode, RunnerApprovalMode::Safe);
    assert_eq!(session.runner_cli.sandbox, RunnerSandboxMode::Enabled);
    assert_eq!(session.runner_cli.plan_mode, RunnerPlanMode::Enabled);
    assert_eq!(
        session.runner_cli.unsupported_option_policy,
        UnsupportedOptionPolicy::Error
    );
}

#[test]
fn continue_session_preserves_phase_type() {
    let planning_session =
        continue_session_with(Runner::Codex, Some("test-session"), PhaseType::Planning);
    assert_eq!(planning_session.phase_type, PhaseType::Planning);

    let implementation_session = continue_session_with(
        Runner::Codex,
        Some("test-session"),
        PhaseType::Implementation,
    );
    assert_eq!(implementation_session.phase_type, PhaseType::Implementation);

    let review_session =
        continue_session_with(Runner::Codex, Some("test-session"), PhaseType::Review);
    assert_eq!(review_session.phase_type, PhaseType::Review);

    let single_phase_session =
        continue_session_with(Runner::Codex, Some("test-session"), PhaseType::SinglePhase);
    assert_eq!(single_phase_session.phase_type, PhaseType::SinglePhase);
}
