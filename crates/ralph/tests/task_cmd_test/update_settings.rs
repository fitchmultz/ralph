//! Purpose: TaskUpdateSettings wiring coverage for task command tests.
//!
//! Responsibilities:
//! - Validate default and populated `TaskUpdateSettings` construction.
//! - Exercise all supported runner, model, and reasoning-effort override permutations.
//!
//! Scope:
//! - Struct construction and field assertions only; no update execution or side-effect testing.
//!
//! Usage:
//! - Uses `super::*;` to access the shared suite imports.
//!
//! Invariants/assumptions callers must respect:
//! - Runner/model/reasoning fields represent overrides, not resolved defaults.

use super::*;

#[test]
fn test_task_update_settings_default_values() {
    let settings = task_cmd::TaskUpdateSettings {
        fields: String::new(),
        runner_override: Some(Runner::Codex),
        model_override: Some(Model::Gpt53Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        dry_run: false,
    };

    assert!(settings.fields.is_empty());
    assert_eq!(settings.runner_override, Some(Runner::Codex));
    assert_eq!(settings.model_override, Some(Model::Gpt53Codex));
    assert!(settings.reasoning_effort_override.is_none());
    assert!(!settings.force);
    assert!(!settings.dry_run);
}

#[test]
fn test_task_update_settings_with_values() {
    let settings = task_cmd::TaskUpdateSettings {
        fields: "scope,evidence,plan".to_string(),
        runner_override: Some(Runner::Opencode),
        model_override: Some(Model::Gpt53),
        reasoning_effort_override: Some(ReasoningEffort::High),
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: true,
        repoprompt_tool_injection: true,
        dry_run: true,
    };

    assert_eq!(settings.fields, "scope,evidence,plan");
    assert_eq!(settings.runner_override, Some(Runner::Opencode));
    assert_eq!(settings.model_override, Some(Model::Gpt53));
    assert!(settings.reasoning_effort_override.is_some());
    assert!(settings.force);
    assert!(settings.repoprompt_tool_injection);
    assert!(settings.dry_run);
}

#[test]
fn test_task_update_settings_all_runners() {
    let runners = vec![
        Runner::Codex,
        Runner::Opencode,
        Runner::Gemini,
        Runner::Claude,
        Runner::Cursor,
        Runner::Kimi,
        Runner::Pi,
    ];

    for runner in runners {
        let settings = task_cmd::TaskUpdateSettings {
            fields: String::new(),
            runner_override: Some(runner),
            model_override: Some(Model::Gpt53Codex),
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            dry_run: false,
        };
        assert!(settings.fields.is_empty());
    }
}

#[test]
fn test_task_update_settings_all_models() {
    let models = vec![
        Model::Gpt53Codex,
        Model::Gpt53,
        Model::Glm47,
        Model::Custom("custom-model".to_string()),
    ];

    for model in models {
        let settings = task_cmd::TaskUpdateSettings {
            fields: String::new(),
            runner_override: Some(Runner::Codex),
            model_override: Some(model),
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            dry_run: false,
        };
        assert!(settings.fields.is_empty());
    }
}

#[test]
fn test_task_update_settings_all_reasoning_efforts() {
    let efforts = vec![
        None,
        Some(ReasoningEffort::Low),
        Some(ReasoningEffort::Medium),
        Some(ReasoningEffort::High),
        Some(ReasoningEffort::XHigh),
    ];

    for effort in efforts {
        let settings = task_cmd::TaskUpdateSettings {
            fields: String::new(),
            runner_override: Some(Runner::Codex),
            model_override: Some(Model::Gpt53Codex),
            reasoning_effort_override: effort,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            dry_run: false,
        };
        assert!(settings.fields.is_empty());
    }
}
