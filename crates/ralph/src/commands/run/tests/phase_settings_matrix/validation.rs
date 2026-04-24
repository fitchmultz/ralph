//! Validation-error coverage for per-phase settings resolution.
//!
//! Purpose:
//! - Validation-error coverage for per-phase settings resolution.
//!
//! Responsibilities:
//! - Assert phase-scoped validation failures for invalid runner/model combinations.
//! - Keep error-message expectations close to the invalid input scenarios.
//!
//! Not handled here:
//! - Success-path precedence behavior.
//! - Integration-style matrix combinations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Invalid phase settings must fail the full matrix resolution.
//! - Error messages stay specific enough to identify the offending phase.

use super::*;

#[test]
fn resolve_phase_settings_invalid_model_for_codex() {
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt53Codex), None);
    let phase_overrides = PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Glm47),
            reasoning_effort: None,
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let err = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3)
        .unwrap_err()
        .to_string();

    assert!(err.contains("Phase 2"));
    assert!(err.contains("invalid model"));
}

#[test]
fn resolve_phase_settings_invalid_custom_model_for_codex() {
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt53Codex), None);
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Custom("invalid-model".to_string())),
            reasoning_effort: None,
        }),
        ..Default::default()
    };
    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let err = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3)
        .unwrap_err()
        .to_string();

    assert!(err.contains("Phase 1"));
}
