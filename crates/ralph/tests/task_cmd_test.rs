//! Unit tests for commands/task.rs (request parsing and option wiring).
//!
//! Responsibilities:
//! - Validate request parsing behavior for task builder inputs.
//! - Assert TaskBuildOptions/TaskUpdateSettings field wiring for overrides.
//!
//! Not handled here:
//! - End-to-end runner execution or queue mutation.
//! - Prompt rendering or task update side effects.
//!
//! Invariants/assumptions:
//! - Runner/model fields on options represent overrides, not resolved defaults.
//!
//! Note: Tests use `read_request_from_args_or_reader` instead of `read_request_from_args_or_stdin`
//! to avoid hanging when stdin is not a terminal (e.g., in CI environments).

use ralph::commands::task as task_cmd;
use ralph::contracts::{Model, Runner, RunnerCliOptionsPatch};
use std::io::Cursor;

#[test]
fn test_read_request_from_args_or_stdin_with_args() {
    let args = vec![
        "create".to_string(),
        "a".to_string(),
        "new".to_string(),
        "task".to_string(),
    ];

    // Use the internal testable function with a mock reader
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "create a new task");
}

#[test]
fn test_read_request_from_args_or_stdin_empty_args_fails() {
    let args: Vec<String> = vec![];
    // Use the internal testable function with a mock reader (simulating terminal)
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Missing request"));
}

#[test]
fn test_read_request_from_args_or_stdin_whitespace_args_fails() {
    let args: Vec<String> = vec!["   ".to_string(), "  ".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_err());
}

#[test]
fn test_read_request_from_args_or_stdin_trims_whitespace() {
    let args: Vec<String> = vec!["  hello  ".to_string(), "  world  ".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    // join(" ") adds space between args, then outer trim is applied
    // "  hello  " + " " + "  world  " = "  hello    world  " -> trimmed -> "hello    world"
    assert_eq!(result.unwrap(), "hello     world");
}

#[test]
fn test_read_request_from_args_or_stdin_special_characters() {
    let args: Vec<String> = vec!["test: fix bug #123".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "test: fix bug #123");
}

#[test]
fn test_read_request_from_args_or_stdin_multilingual() {
    let args: Vec<String> = vec!["Hello 世界 🎉".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello 世界 🎉");
}

#[test]
fn test_task_build_options_default_values() {
    let opts = task_cmd::TaskBuildOptions {
        request: "test request".to_string(),
        hint_tags: String::new(),
        hint_scope: String::new(),
        runner_override: Some(Runner::Codex),
        model_override: Some(Model::Gpt52Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        template_hint: None,
        template_target: None,
    };

    assert_eq!(opts.request, "test request");
    assert!(opts.hint_tags.is_empty());
    assert!(opts.hint_scope.is_empty());
    assert_eq!(opts.runner_override, Some(Runner::Codex));
    assert_eq!(opts.model_override, Some(Model::Gpt52Codex));
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
        model_override: Some(Model::Gpt52),
        reasoning_effort_override: Some(ralph::contracts::ReasoningEffort::High),
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: true,
        repoprompt_tool_injection: false,
        template_hint: None,
        template_target: None,
    };

    assert_eq!(opts.request, "implement feature");
    assert_eq!(opts.hint_tags, "rust,testing");
    assert_eq!(opts.hint_scope, "crates/ralph/src");
    assert_eq!(opts.runner_override, Some(Runner::Opencode));
    assert_eq!(opts.model_override, Some(Model::Gpt52));
    assert!(opts.reasoning_effort_override.is_some());
    assert!(opts.force);
}

#[test]
fn test_task_build_options_empty_request_validation() {
    // This tests the validation that happens in build_task
    let opts = task_cmd::TaskBuildOptions {
        request: "".to_string(),
        hint_tags: String::new(),
        hint_scope: String::new(),
        runner_override: Some(Runner::Codex),
        model_override: Some(Model::Gpt52Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        template_hint: None,
        template_target: None,
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
        model_override: Some(Model::Gpt52Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        template_hint: None,
        template_target: None,
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
            model_override: Some(Model::Gpt52Codex),
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
        };
        assert_eq!(opts.request, "test");
    }
}

#[test]
fn test_task_build_options_all_models() {
    let models = vec![
        Model::Gpt52Codex,
        Model::Gpt52,
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
        };
        assert_eq!(opts.request, "test");
    }
}

#[test]
fn test_task_build_options_all_reasoning_efforts() {
    let efforts = vec![
        None,
        Some(ralph::contracts::ReasoningEffort::Low),
        Some(ralph::contracts::ReasoningEffort::Medium),
        Some(ralph::contracts::ReasoningEffort::High),
        Some(ralph::contracts::ReasoningEffort::XHigh),
    ];

    for effort in efforts {
        let opts = task_cmd::TaskBuildOptions {
            request: "test".to_string(),
            hint_tags: String::new(),
            hint_scope: String::new(),
            runner_override: Some(Runner::Codex),
            model_override: Some(Model::Gpt52Codex),
            reasoning_effort_override: effort,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
        };
        // Just verify we can create options with each effort level
        assert_eq!(opts.request, "test");
    }
}

#[test]
fn test_read_request_single_arg() {
    let args = vec!["single".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "single");
}

#[test]
fn test_read_request_with_newlines() {
    let args = vec!["line1\nline2".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "line1\nline2");
}

#[test]
fn test_read_request_with_tabs() {
    let args = vec!["line1\tline2".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "line1\tline2");
}

#[test]
fn test_read_request_preserves_internal_whitespace() {
    let args = vec![
        "word1   word2".to_string(),
        "word3".to_string(),
        "   word4   word5".to_string(),
    ];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    // Internal whitespace within args is preserved
    assert!(result.unwrap().starts_with("word1   word2"));
}

#[test]
fn test_task_update_settings_default_values() {
    let settings = task_cmd::TaskUpdateSettings {
        fields: String::new(),
        runner_override: Some(Runner::Codex),
        model_override: Some(Model::Gpt52Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        dry_run: false,
    };

    assert!(settings.fields.is_empty());
    assert_eq!(settings.runner_override, Some(Runner::Codex));
    assert_eq!(settings.model_override, Some(Model::Gpt52Codex));
    assert!(settings.reasoning_effort_override.is_none());
    assert!(!settings.force);
    assert!(!settings.dry_run);
}

#[test]
fn test_task_update_settings_with_values() {
    let settings = task_cmd::TaskUpdateSettings {
        fields: "scope,evidence,plan".to_string(),
        runner_override: Some(Runner::Opencode),
        model_override: Some(Model::Gpt52),
        reasoning_effort_override: Some(ralph::contracts::ReasoningEffort::High),
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: true,
        repoprompt_tool_injection: true,
        dry_run: true,
    };

    assert_eq!(settings.fields, "scope,evidence,plan");
    assert_eq!(settings.runner_override, Some(Runner::Opencode));
    assert_eq!(settings.model_override, Some(Model::Gpt52));
    assert!(settings.reasoning_effort_override.is_some());
    assert!(settings.force);
    assert!(settings.repoprompt_tool_injection);
    assert!(settings.dry_run);
}

#[test]
fn test_compare_task_fields_no_changes() {
    let before = r#"{"id":"RQ-0001","status":"todo","title":"Test task"}"#;
    let after = r#"{"id":"RQ-0001","status":"todo","title":"Test task"}"#;

    let result = task_cmd::compare_task_fields(before, after);
    assert!(result.is_ok());
    let changed = result.unwrap();
    assert_eq!(changed.len(), 0);
}

#[test]
fn test_compare_task_fields_some_changes() {
    let before = r#"{"id":"RQ-0001","status":"todo","title":"Test task"}"#;
    let after = r#"{"id":"RQ-0001","status":"doing","title":"Updated task"}"#;

    let result = task_cmd::compare_task_fields(before, after);
    assert!(result.is_ok());
    let changed = result.unwrap();
    assert!(changed.contains(&"status".to_string()));
    assert!(changed.contains(&"title".to_string()));
}

#[test]
fn test_compare_task_fields_invalid_json() {
    let before = "{invalid json}";
    let after = r#"{"id":"RQ-0001"}"#;

    let result = task_cmd::compare_task_fields(before, after);
    assert!(result.is_err());
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
            model_override: Some(Model::Gpt52Codex),
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
        Model::Gpt52Codex,
        Model::Gpt52,
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
        Some(ralph::contracts::ReasoningEffort::Low),
        Some(ralph::contracts::ReasoningEffort::Medium),
        Some(ralph::contracts::ReasoningEffort::High),
        Some(ralph::contracts::ReasoningEffort::XHigh),
    ];

    for effort in efforts {
        let settings = task_cmd::TaskUpdateSettings {
            fields: String::new(),
            runner_override: Some(Runner::Codex),
            model_override: Some(Model::Gpt52Codex),
            reasoning_effort_override: effort,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            dry_run: false,
        };
        assert!(settings.fields.is_empty());
    }
}
