//! Precedence-chain coverage for per-phase settings resolution.
//!
//! Purpose:
//! - Precedence-chain coverage for per-phase settings resolution.
//!
//! Responsibilities:
//! - Verify CLI, task, config, and code-default precedence for phase settings.
//! - Keep override ordering assertions localized to one matrix-focused module.
//!
//! Not handled here:
//! - Model defaulting details.
//! - Validation-error or warning-specific coverage.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Higher-precedence override layers must win without changing untouched phases.
//! - Successful resolution returns a fully populated matrix for the requested phase count.

use super::*;

#[test]
fn resolve_phase_settings_cli_phase_override_beats_global() {
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt53), None);
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(
        Some(Runner::Opencode),
        Some(Model::Glm47),
        Some(ReasoningEffort::High),
        Some(phase_overrides),
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt53Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::Low));
}

#[test]
fn resolve_phase_settings_config_phase_override_beats_global() {
    let mut config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt53), None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: Some(Model::Custom("gemini-pro".to_string())),
            reasoning_effort: None,
        }),
        ..Default::default()
    });

    let overrides = test_overrides_with_phases(
        Some(Runner::Codex),
        Some(Model::Gpt53Codex),
        Some(ReasoningEffort::High),
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase2.runner, Runner::Gemini);
    assert_eq!(matrix.phase2.model.as_str(), "gemini-pro");
}

#[test]
fn resolve_phase_settings_task_phase_override_beats_config_phase_override() {
    let mut config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt53), None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
        ..Default::default()
    });

    let task_agent = TaskAgent {
        runner: None,
        model: None,
        model_effort: ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: Some(PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Kimi),
                model: Some(Model::Custom("kimi-code/kimi-for-coding".to_string())),
                reasoning_effort: Some(ReasoningEffort::High),
            }),
            ..Default::default()
        }),
    };

    let overrides = AgentOverrides::default();
    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Kimi);
    assert_eq!(matrix.phase1.model.as_str(), "kimi-code/kimi-for-coding");
}

#[test]
fn resolve_phase_settings_cli_phase_override_beats_task_phase_override() {
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt53), None);
    let task_agent = TaskAgent {
        runner: None,
        model: None,
        model_effort: ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: Some(PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Kimi),
                model: Some(Model::Custom("kimi-code/kimi-for-coding".to_string())),
                reasoning_effort: None,
            }),
            ..Default::default()
        }),
    };
    let overrides = test_overrides_with_phases(
        None,
        None,
        None,
        Some(PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Codex),
                model: Some(Model::Gpt53Codex),
                reasoning_effort: Some(ReasoningEffort::High),
            }),
            ..Default::default()
        }),
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt53Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
}

#[test]
fn resolve_phase_settings_cli_global_beats_task() {
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt53), None);
    let task_agent = test_task_agent(
        Some(Runner::Opencode),
        Some(Model::Glm47),
        ModelEffort::High,
    );
    let overrides = test_overrides_with_phases(
        Some(Runner::Codex),
        Some(Model::Gpt53Codex),
        Some(ReasoningEffort::Medium),
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt53Codex);
    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase3.runner, Runner::Codex);
}

#[test]
fn resolve_phase_settings_task_beats_config() {
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt53), None);
    let task_agent = test_task_agent(
        Some(Runner::Opencode),
        Some(Model::Glm47),
        ModelEffort::High,
    );

    let (matrix, _warnings) = resolve_phase_settings_matrix(
        &AgentOverrides::default(),
        &config_agent,
        Some(&task_agent),
        3,
    )
    .unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
}

#[test]
fn resolve_phase_settings_config_beats_default() {
    let config_agent = test_config_agent(
        Some(Runner::Gemini),
        Some(Model::Custom("gemini-custom".to_string())),
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&AgentOverrides::default(), &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Gemini);
    assert_eq!(matrix.phase1.model.as_str(), "gemini-custom");
}

#[test]
fn resolve_phase_settings_uses_code_default_when_nothing_specified() {
    let config_agent = crate::contracts::AgentConfig::default();
    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&AgentOverrides::default(), &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Claude);
}
