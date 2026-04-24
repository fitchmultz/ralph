//! Integration-style coverage for per-phase settings resolution.
//!
//! Purpose:
//! - Integration-style coverage for per-phase settings resolution.
//!
//! Responsibilities:
//! - Exercise mixed runner/model/effort combinations across all phases.
//! - Verify config-phase defaults and CLI phase overrides together.
//!
//! Not handled here:
//! - Narrow precedence-unit scenarios.
//! - Dedicated validation-error assertions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Complex matrices preserve phase-local runner/model behavior.
//! - CLI phase overrides continue to beat config phase overrides.

use super::*;

#[test]
fn resolve_phase_settings_full_matrix_resolution() {
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        Some(ReasoningEffort::Medium),
    );
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: Some(Model::Custom("gemini-pro".to_string())),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt53Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));

    assert_eq!(matrix.phase2.runner, Runner::Opencode);
    assert_eq!(matrix.phase2.model, Model::Glm47);
    assert_eq!(matrix.phase2.reasoning_effort, None);

    assert_eq!(matrix.phase3.runner, Runner::Gemini);
    assert_eq!(matrix.phase3.model.as_str(), "gemini-pro");
    assert_eq!(matrix.phase3.reasoning_effort, None);
}

#[test]
fn resolve_phase_settings_config_phase_overrides_only() {
    let mut config_agent = test_config_agent(Some(Runner::Claude), None, None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: None,
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    });

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&AgentOverrides::default(), &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt53Codex);
    assert_eq!(matrix.phase2.runner, Runner::Claude);
    assert_eq!(matrix.phase3.runner, Runner::Gemini);
}

#[test]
fn resolve_phase_settings_cli_overrides_config_phase() {
    let mut config_agent = test_config_agent(Some(Runner::Claude), None, None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
        ..Default::default()
    });

    let cli_phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: Some(Model::Glm47),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(cli_phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
}
