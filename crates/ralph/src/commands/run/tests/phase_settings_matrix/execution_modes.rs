//! Phase-count and warning coverage for per-phase settings resolution.
//!
//! Purpose:
//! - Phase-count and warning coverage for per-phase settings resolution.
//!
//! Responsibilities:
//! - Verify single-pass, two-phase, and three-phase mapping behavior.
//! - Assert warning flags for overrides that become unused under each mode.
//!
//! Not handled here:
//! - Override-precedence ordering.
//! - Invalid-model validation behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Single-pass execution resolves through Phase 2 settings.
//! - Warning flags precisely track which configured overrides were ignored.

use super::*;

#[test]
fn resolve_phase_settings_single_pass_uses_phase2_overrides() {
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt53), None);
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: Some(Model::Glm47),
            reasoning_effort: None,
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            reasoning_effort: Some(ReasoningEffort::High),
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

    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase2.model, Model::Gpt53Codex);
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::High));
    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);
}

#[test]
fn resolve_phase_settings_two_phase_warns_about_phase3() {
    let phase_overrides = PhaseOverrides {
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();

    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);
}

#[test]
fn resolve_phase_settings_warns_unused_phase3_when_phases_is_2() {
    let phase_overrides = PhaseOverrides {
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();

    assert!(warnings.unused_phase3);
    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
}

#[test]
fn resolve_phase_settings_warns_unused_task_phase3_override_when_phases_is_2() {
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);
    let task_agent = TaskAgent {
        runner: None,
        model: None,
        model_effort: ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: Some(PhaseOverrides {
            phase3: Some(PhaseOverrideConfig {
                runner: Some(Runner::Gemini),
                model: Some(Model::Custom("gemini-3-pro-preview".to_string())),
                reasoning_effort: None,
            }),
            ..Default::default()
        }),
    };

    let (_matrix, warnings) = resolve_phase_settings_matrix(
        &AgentOverrides::default(),
        &config_agent,
        Some(&task_agent),
        2,
    )
    .unwrap();

    assert!(warnings.unused_phase3);
}

#[test]
fn resolve_phase_settings_warns_unused_phase1_and_phase3_when_phases_is_1() {
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();

    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);
}
