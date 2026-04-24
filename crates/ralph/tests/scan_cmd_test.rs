//! Unit tests for commands/scan.rs (scan option wiring and focus handling).
//!
//! Purpose:
//! - Unit tests for commands/scan.rs (scan option wiring and focus handling).
//!
//! Responsibilities:
//! - Validate ScanOptions construction behavior and override wiring.
//! - Ensure focus text handling covers common edge cases.
//!
//! Not handled here:
//! - End-to-end scan execution or queue mutation.
//! - Runner invocation behavior or prompt rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - ScanOptions runner/model fields represent overrides, not resolved defaults.

use ralph::cli::scan::ScanMode;
use ralph::commands::scan as scan_cmd;
use ralph::contracts::{GitRevertMode, Model, Runner, RunnerCliOptionsPatch};

fn base_scan_options() -> scan_cmd::ScanOptions {
    scan_cmd::ScanOptions {
        focus: String::new(),
        mode: ScanMode::Maintenance,
        runner_override: Some(Runner::Codex),
        model_override: Some(Model::Gpt53Codex),
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        git_revert_mode: GitRevertMode::Ask,
        lock_mode: scan_cmd::ScanLockMode::Acquire,
        output_handler: None,
        revert_prompt: None,
    }
}

#[test]
fn test_scan_options_default_values() {
    let opts = base_scan_options();

    assert!(opts.focus.is_empty());
    assert_eq!(opts.runner_override, Some(Runner::Codex));
    assert_eq!(opts.model_override, Some(Model::Gpt53Codex));
    assert!(opts.reasoning_effort_override.is_none());
    assert!(!opts.force);
}

#[test]
fn test_scan_options_with_focus() {
    let opts = scan_cmd::ScanOptions {
        focus: "test coverage".to_string(),
        ..base_scan_options()
    };

    assert_eq!(opts.focus, "test coverage");
}

#[test]
fn test_scan_options_all_runners() {
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
        let opts = scan_cmd::ScanOptions {
            focus: "test".to_string(),
            runner_override: Some(runner),
            ..base_scan_options()
        };
        assert_eq!(opts.focus, "test");
    }
}

#[test]
fn test_scan_options_all_models() {
    let models = vec![
        Model::Gpt53Codex,
        Model::Gpt53,
        Model::Glm47,
        Model::Custom("custom-model".to_string()),
    ];

    for model in models {
        let opts = scan_cmd::ScanOptions {
            focus: "test".to_string(),
            runner_override: Some(Runner::Codex),
            model_override: Some(model),
            ..base_scan_options()
        };
        assert_eq!(opts.focus, "test");
    }
}

#[test]
fn test_scan_options_all_reasoning_efforts() {
    let efforts = vec![
        None,
        Some(ralph::contracts::ReasoningEffort::Low),
        Some(ralph::contracts::ReasoningEffort::Medium),
        Some(ralph::contracts::ReasoningEffort::High),
        Some(ralph::contracts::ReasoningEffort::XHigh),
    ];

    for effort in efforts {
        let opts = scan_cmd::ScanOptions {
            focus: "test".to_string(),
            reasoning_effort_override: effort,
            ..base_scan_options()
        };
        assert_eq!(opts.focus, "test");
    }
}

#[test]
fn test_scan_options_with_force() {
    let opts = scan_cmd::ScanOptions {
        focus: "security".to_string(),
        force: true,
        ..base_scan_options()
    };

    assert!(opts.force);
}

#[test]
fn test_scan_options_various_focus_areas() {
    let focus_areas = vec![
        "test coverage",
        "security vulnerabilities",
        "performance optimizations",
        "code quality",
        "documentation",
        "error handling",
        "memory leaks",
        "API design",
    ];

    for focus in focus_areas {
        let opts = scan_cmd::ScanOptions {
            focus: focus.to_string(),
            ..base_scan_options()
        };
        assert_eq!(opts.focus, focus);
    }
}

#[test]
fn test_scan_options_empty_focus() {
    let opts = scan_cmd::ScanOptions {
        ..base_scan_options()
    };

    assert!(opts.focus.is_empty());
    // Empty focus is valid - means scan entire codebase
}

#[test]
fn test_scan_options_whitespace_focus() {
    let opts = scan_cmd::ScanOptions {
        focus: "   ".to_string(),
        ..base_scan_options()
    };

    assert!(!opts.focus.is_empty());
    assert_eq!(opts.focus, "   ");
}

#[test]
fn test_scan_options_special_characters_in_focus() {
    let special_focuses = vec![
        "fix: bug #123",
        "TODO: implement feature X",
        "FIXME: race condition",
        "HACK: synthetic workaround",
        "NOTE: review this code",
        "XXX: needs refactoring",
    ];

    for focus in special_focuses {
        let opts = scan_cmd::ScanOptions {
            focus: focus.to_string(),
            ..base_scan_options()
        };
        assert_eq!(opts.focus, focus);
    }
}

#[test]
fn test_scan_options_multilingual_focus() {
    let multilingual_focuses = vec![
        "internationalization i18n",
        "localization l10n",
        "unicode support",
        "UTF-8 encoding",
        "多语言支持",
    ];

    for focus in multilingual_focuses {
        let opts = scan_cmd::ScanOptions {
            focus: focus.to_string(),
            ..base_scan_options()
        };
        assert_eq!(opts.focus, focus);
    }
}

#[test]
fn test_scan_options_with_specific_file_paths() {
    let file_focuses = vec![
        "crates/ralph/src/*.rs",
        "src/**/*.ts",
        "tests/**/*_test.rs",
        ".github/workflows/*.yml",
    ];

    for focus in file_focuses {
        let opts = scan_cmd::ScanOptions {
            focus: focus.to_string(),
            ..base_scan_options()
        };
        assert_eq!(opts.focus, focus);
    }
}
