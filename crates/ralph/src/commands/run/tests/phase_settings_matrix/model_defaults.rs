//! Model-default and effort-handling coverage for per-phase settings resolution.
//!
//! Purpose:
//! - Model-default and effort-handling coverage for per-phase settings resolution.
//!
//! Responsibilities:
//! - Verify runner-specific model defaulting behavior.
//! - Assert reasoning-effort handling for supported and unsupported phases.
//!
//! Not handled here:
//! - Override-precedence ordering.
//! - Execution-mode warning coverage.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Runner changes without explicit models must fall back to that runner's default model.
//! - Phases for runners without reasoning controls ignore reasoning-effort inputs.

use super::*;

#[test]
fn resolve_phase_settings_runner_override_uses_default_model() {
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );
    let overrides = test_overrides_with_phases(Some(Runner::Opencode), None, None, None);

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
}

#[test]
fn resolve_phase_settings_phase_runner_override_uses_default_model() {
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
    assert_eq!(matrix.phase2.runner, Runner::Claude);
}

#[test]
fn resolve_phase_settings_explicit_model_preserved_with_runner_override() {
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );
    let phase_overrides = PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53),
            reasoning_effort: None,
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase2.model, Model::Gpt53);
}

#[test]
fn resolve_phase_settings_effort_some_for_codex() {
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt53Codex), None);
    let overrides = test_overrides_with_phases(None, None, Some(ReasoningEffort::High), None);

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::High));
}

#[test]
fn resolve_phase_settings_effort_some_for_pi() {
    let config_agent = test_config_agent(
        Some(Runner::Pi),
        Some(Model::Custom("openai-codex/gpt-5.5".to_string())),
        None,
    );
    let overrides = test_overrides_with_phases(None, None, Some(ReasoningEffort::High), None);

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::High));
}

#[test]
fn resolve_phase_settings_effort_none_for_unsupported_runner() {
    let config_agent = test_config_agent(Some(Runner::Opencode), Some(Model::Glm47), None);
    let overrides = test_overrides_with_phases(None, None, Some(ReasoningEffort::High), None);

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.reasoning_effort, None);
    assert_eq!(matrix.phase2.reasoning_effort, None);
}

#[test]
fn resolve_phase_settings_effort_precedence_within_codex() {
    let config_agent = test_config_agent(
        Some(Runner::Codex),
        Some(Model::Gpt53Codex),
        Some(ReasoningEffort::Low),
    );
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::Low));
}
