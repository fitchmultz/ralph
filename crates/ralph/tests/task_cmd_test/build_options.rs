//! Purpose: TaskBuildOptions wiring coverage for task command tests.
//!
//! Responsibilities:
//! - Validate default and populated `TaskBuildOptions` construction.
//! - Cover empty/whitespace request validation assumptions.
//! - Exercise all supported runner, model, and reasoning-effort override permutations.
//!
//! Scope:
//! - Struct construction and field assertions only; no runner execution or queue mutation.
//!
//! Usage:
//! - Uses `super::*;` to access the shared suite imports.
//!
//! Invariants/assumptions callers must respect:
//! - Runner/model/reasoning fields represent overrides, not resolved defaults.

use super::*;

#[test]
fn test_task_build_options_default_values() {
    let opts = task_cmd::TaskBuildOptions {
        request: "test request".to_string(),
        hint_tags: String::new(),
        hint_scope: String::new(),
        runner_override: Some(Runner::Codex),
        model_override: Some(Model::Gpt53Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        template_hint: None,
        template_target: None,
        strict_templates: false,
        estimated_minutes: None,
    };

    assert_eq!(opts.request, "test request");
    assert!(opts.hint_tags.is_empty());
    assert!(opts.hint_scope.is_empty());
    assert_eq!(opts.runner_override, Some(Runner::Codex));
    assert_eq!(opts.model_override, Some(Model::Gpt53Codex));
    assert!(opts.reasoning_effort_override.is_none());
    assert!(!opts.force);
}

#[test]
fn test_task_build_options_with_values() {
    let opts = task_cmd::TaskBuildOptions {
        request: "implement feature".to_string(),
        hint_tags: "rust,testing".to_string(),
        hint_scope: "crates/ralph/src".to_string(),
        runner_override: Some(Runner::Opencode),
        model_override: Some(Model::Gpt53),
        reasoning_effort_override: Some(ReasoningEffort::High),
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: true,
        repoprompt_tool_injection: false,
        template_hint: None,
        template_target: None,
        strict_templates: false,
        estimated_minutes: None,
    };

    assert_eq!(opts.request, "implement feature");
    assert_eq!(opts.hint_tags, "rust,testing");
    assert_eq!(opts.hint_scope, "crates/ralph/src");
    assert_eq!(opts.runner_override, Some(Runner::Opencode));
    assert_eq!(opts.model_override, Some(Model::Gpt53));
    assert!(opts.reasoning_effort_override.is_some());
    assert!(opts.force);
}

#[test]
fn test_task_build_options_empty_request_validation() {
    let opts = task_cmd::TaskBuildOptions {
        request: "".to_string(),
        hint_tags: String::new(),
        hint_scope: String::new(),
        runner_override: Some(Runner::Codex),
        model_override: Some(Model::Gpt53Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        template_hint: None,
        template_target: None,
        strict_templates: false,
        estimated_minutes: None,
    };

    assert!(opts.request.trim().is_empty());
}

#[test]
fn test_task_build_options_whitespace_request_validation() {
    let opts = task_cmd::TaskBuildOptions {
        request: "   ".to_string(),
        hint_tags: String::new(),
        hint_scope: String::new(),
        runner_override: Some(Runner::Codex),
        model_override: Some(Model::Gpt53Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        template_hint: None,
        template_target: None,
        strict_templates: false,
        estimated_minutes: None,
    };

    assert!(opts.request.trim().is_empty());
}

#[test]
fn test_task_build_options_all_runners() {
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
        let opts = task_cmd::TaskBuildOptions {
            request: "test".to_string(),
            hint_tags: String::new(),
            hint_scope: String::new(),
            runner_override: Some(runner),
            model_override: Some(Model::Gpt53Codex),
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
            strict_templates: false,
            estimated_minutes: None,
        };
        assert_eq!(opts.request, "test");
    }
}

#[test]
fn test_task_build_options_all_models() {
    let models = vec![
        Model::Gpt53Codex,
        Model::Gpt53,
        Model::Glm47,
        Model::Custom("custom-model".to_string()),
    ];

    for model in models {
        let opts = task_cmd::TaskBuildOptions {
            request: "test".to_string(),
            hint_tags: String::new(),
            hint_scope: String::new(),
            runner_override: Some(Runner::Codex),
            model_override: Some(model),
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
            strict_templates: false,
            estimated_minutes: None,
        };
        assert_eq!(opts.request, "test");
    }
}

#[test]
fn test_task_build_options_all_reasoning_efforts() {
    let efforts = vec![
        None,
        Some(ReasoningEffort::Low),
        Some(ReasoningEffort::Medium),
        Some(ReasoningEffort::High),
        Some(ReasoningEffort::XHigh),
    ];

    for effort in efforts {
        let opts = task_cmd::TaskBuildOptions {
            request: "test".to_string(),
            hint_tags: String::new(),
            hint_scope: String::new(),
            runner_override: Some(Runner::Codex),
            model_override: Some(Model::Gpt53Codex),
            reasoning_effort_override: effort,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
            strict_templates: false,
            estimated_minutes: None,
        };
        assert_eq!(opts.request, "test");
    }
}
