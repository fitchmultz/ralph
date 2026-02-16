//! Per-phase settings wiring tests (RQ-0496).

use super::{test_config_agent, test_overrides_with_phases};
use crate::agent::AgentOverrides;
use crate::contracts::{Model, PhaseOverrideConfig, PhaseOverrides, ReasoningEffort, Runner};
use crate::runner::{ResolvedPhaseSettings, resolve_phase_settings_matrix};

/// Test that ResolvedPhaseSettings can be converted to AgentSettings
#[test]
fn resolved_phase_settings_to_agent_settings_conversion() {
    let phase_settings = ResolvedPhaseSettings {
        runner: Runner::Codex,
        model: Model::Gpt52Codex,
        reasoning_effort: Some(ReasoningEffort::High),
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
    };

    let agent_settings = phase_settings.to_agent_settings();

    assert_eq!(agent_settings.runner, Runner::Codex);
    assert_eq!(agent_settings.model, Model::Gpt52Codex);
    assert_eq!(agent_settings.reasoning_effort, Some(ReasoningEffort::High));
}

/// Test that different runners in different phases are properly resolved
#[test]
fn per_phase_settings_different_runners_per_phase() {
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: Some(Model::Glm47),
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: Some(Model::Custom("gemini-pro".to_string())),
            reasoning_effort: None,
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1: Codex
    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));

    // Phase 2: Opencode
    assert_eq!(matrix.phase2.runner, Runner::Opencode);
    assert_eq!(matrix.phase2.model, Model::Glm47);
    assert_eq!(matrix.phase2.reasoning_effort, None);

    // Phase 3: Gemini
    assert_eq!(matrix.phase3.runner, Runner::Gemini);
    assert_eq!(matrix.phase3.model.as_str(), "gemini-pro");
    assert_eq!(matrix.phase3.reasoning_effort, None);
}

/// Test that single-pass mode (phases=1) uses Phase 2 settings
#[test]
fn single_pass_uses_phase2_settings() {
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: None,
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: Some(Model::Glm47),
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();

    // Phase 2 settings should be resolved for single-pass
    assert_eq!(matrix.phase2.runner, Runner::Opencode);
    assert_eq!(matrix.phase2.model, Model::Glm47);

    // Phase 1 and Phase 3 overrides are unused
    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2); // Phase 2 is used
    assert!(warnings.unused_phase3);
}

/// Test that Phase 2 settings are always resolved (used in all modes)
#[test]
fn phase2_settings_always_resolved() {
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);
    let overrides = AgentOverrides::default();

    // Test with phases=1
    let (matrix, _) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();
    assert_eq!(matrix.phase2.runner, Runner::Claude);

    // Test with phases=2
    let (matrix, _) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();
    assert_eq!(matrix.phase2.runner, Runner::Claude);

    // Test with phases=3
    let (matrix, _) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();
    assert_eq!(matrix.phase2.runner, Runner::Claude);
}

/// Test that warnings are properly collected for unused phases
#[test]
fn resolution_warnings_collected_correctly() {
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: None,
            reasoning_effort: None,
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    // With phases=1: phase1 and phase3 are unused
    let (_, warnings) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();
    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);

    // With phases=2: only phase3 is unused
    let (_, warnings) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();
    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);

    // With phases=3: no unused phases
    let (_, warnings) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();
    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(!warnings.unused_phase3);
}
