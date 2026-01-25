//! Unit tests for runutil.rs (runner invocation and state transitions).

use ralph::contracts::{ClaudePermissionMode, Model, ReasoningEffort, Runner};
use ralph::runutil;
use std::time::Duration;

#[test]
fn test_runner_invocation_creation() {
    let temp_dir = std::env::temp_dir();
    let repo_root = temp_dir.as_path();

    let invocation = runutil::RunnerInvocation {
        repo_root,
        runner_kind: Runner::Codex,
        bins: ralph::runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: Some(ReasoningEffort::Medium),
        prompt: "test prompt",
        timeout: None,
        permission_mode: None,
        revert_on_error: true,
        git_revert_mode: ralph::contracts::GitRevertMode::Ask,
        output_handler: None,
        output_stream: ralph::runner::OutputStream::Terminal,
        revert_prompt: None,
    };

    assert_eq!(invocation.runner_kind, Runner::Codex);
    assert_eq!(invocation.model, Model::Gpt52Codex);
    assert_eq!(invocation.reasoning_effort, Some(ReasoningEffort::Medium));
    assert_eq!(invocation.prompt, "test prompt");
    assert!(invocation.timeout.is_none());
    assert!(invocation.revert_on_error);
}

#[test]
fn test_runner_invocation_with_timeout() {
    let temp_dir = std::env::temp_dir();
    let repo_root = temp_dir.as_path();

    let invocation = runutil::RunnerInvocation {
        repo_root,
        runner_kind: Runner::Codex,
        bins: ralph::runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        prompt: "test prompt",
        timeout: Some(Duration::from_secs(60)),
        permission_mode: None,
        revert_on_error: false,
        git_revert_mode: ralph::contracts::GitRevertMode::Ask,
        output_handler: None,
        output_stream: ralph::runner::OutputStream::Terminal,
        revert_prompt: None,
    };

    assert_eq!(invocation.timeout, Some(Duration::from_secs(60)));
    assert!(!invocation.revert_on_error);
}

#[test]
fn test_runner_invocation_all_runners() {
    let temp_dir = std::env::temp_dir();
    let repo_root = temp_dir.as_path();

    let runners = vec![
        Runner::Codex,
        Runner::Opencode,
        Runner::Gemini,
        Runner::Claude,
    ];

    for runner in runners {
        let invocation = runutil::RunnerInvocation {
            repo_root,
            runner_kind: runner,
            bins: ralph::runner::RunnerBinaries {
                codex: "codex",
                opencode: "opencode",
                gemini: "gemini",
                claude: "claude",
            },
            model: Model::Gpt52Codex,
            reasoning_effort: None,
            prompt: "test",
            timeout: None,
            permission_mode: None,
            revert_on_error: false,
            git_revert_mode: ralph::contracts::GitRevertMode::Ask,
            output_handler: None,
            output_stream: ralph::runner::OutputStream::Terminal,
            revert_prompt: None,
        };
        assert_eq!(invocation.runner_kind, runner);
    }
}

#[test]
fn test_runner_invocation_all_models() {
    let temp_dir = std::env::temp_dir();
    let repo_root = temp_dir.as_path();

    let models = vec![
        Model::Gpt52Codex,
        Model::Gpt52,
        Model::Glm47,
        Model::Custom("custom".to_string()),
    ];

    for model in models {
        let invocation = runutil::RunnerInvocation {
            repo_root,
            runner_kind: Runner::Codex,
            bins: ralph::runner::RunnerBinaries {
                codex: "codex",
                opencode: "opencode",
                gemini: "gemini",
                claude: "claude",
            },
            model: model.clone(),
            reasoning_effort: None,
            prompt: "test",
            timeout: None,
            permission_mode: None,
            revert_on_error: false,
            git_revert_mode: ralph::contracts::GitRevertMode::Ask,
            output_handler: None,
            output_stream: ralph::runner::OutputStream::Terminal,
            revert_prompt: None,
        };
        assert_eq!(invocation.model, model);
    }
}

#[test]
fn test_runner_invocation_all_permission_modes() {
    let temp_dir = std::env::temp_dir();
    let repo_root = temp_dir.as_path();

    let modes = vec![
        Some(ClaudePermissionMode::AcceptEdits),
        Some(ClaudePermissionMode::BypassPermissions),
        None,
    ];

    for mode in modes {
        let invocation = runutil::RunnerInvocation {
            repo_root,
            runner_kind: Runner::Claude,
            bins: ralph::runner::RunnerBinaries {
                codex: "codex",
                opencode: "opencode",
                gemini: "gemini",
                claude: "claude",
            },
            model: Model::Gpt52,
            reasoning_effort: None,
            prompt: "test",
            timeout: None,
            permission_mode: mode,
            revert_on_error: false,
            git_revert_mode: ralph::contracts::GitRevertMode::Ask,
            output_handler: None,
            output_stream: ralph::runner::OutputStream::Terminal,
            revert_prompt: None,
        };
        assert_eq!(invocation.permission_mode, mode);
    }
}

#[test]
fn test_runner_error_messages_creation() {
    let messages = runutil::RunnerErrorMessages {
        log_label: "test_runner",
        interrupted_msg: "Test interrupted",
        timeout_msg: "Test timed out",
        terminated_msg: "Test terminated",
        non_zero_msg: |code| format!("Non-zero exit: {}", code),
        other_msg: |err| format!("Other error: {:?}", err),
    };

    assert_eq!(messages.log_label, "test_runner");
    assert_eq!(messages.interrupted_msg, "Test interrupted");
    assert_eq!(messages.timeout_msg, "Test timed out");
    assert_eq!(messages.terminated_msg, "Test terminated");
}

#[test]
fn test_runner_error_messages_non_zero_callback() {
    let messages = runutil::RunnerErrorMessages {
        log_label: "test",
        interrupted_msg: "interrupted",
        timeout_msg: "timeout",
        terminated_msg: "terminated",
        non_zero_msg: |code| format!("Exit code: {}", code),
        other_msg: |_| "other".to_string(),
    };

    let msg = (messages.non_zero_msg)(42);
    assert_eq!(msg, "Exit code: 42");
}

#[test]
fn test_runner_error_messages_other_callback() {
    let messages = runutil::RunnerErrorMessages {
        log_label: "test",
        interrupted_msg: "interrupted",
        timeout_msg: "timeout",
        terminated_msg: "terminated",
        non_zero_msg: |_| "non-zero".to_string(),
        other_msg: |err| format!("Error: {}", err),
    };

    // Create a dummy RunnerError - we'll just use Interrupted for testing the callback
    let fake_err = ralph::runner::RunnerError::Interrupted;
    let msg = (messages.other_msg)(fake_err);
    assert!(msg.contains("Error:"));
}

#[test]
fn test_runner_invocation_various_timeouts() {
    let temp_dir = std::env::temp_dir();
    let repo_root = temp_dir.as_path();

    let timeouts = vec![
        Some(Duration::from_secs(30)),
        Some(Duration::from_secs(60)),
        Some(Duration::from_secs(120)),
        Some(Duration::from_secs(300)),
        Some(Duration::from_millis(5000)),
        None,
    ];

    for timeout in timeouts {
        let invocation = runutil::RunnerInvocation {
            repo_root,
            runner_kind: Runner::Codex,
            bins: ralph::runner::RunnerBinaries {
                codex: "codex",
                opencode: "opencode",
                gemini: "gemini",
                claude: "claude",
            },
            model: Model::Gpt52Codex,
            reasoning_effort: None,
            prompt: "test",
            timeout,
            permission_mode: None,
            revert_on_error: false,
            git_revert_mode: ralph::contracts::GitRevertMode::Ask,
            output_handler: None,
            output_stream: ralph::runner::OutputStream::Terminal,
            revert_prompt: None,
        };
        assert_eq!(invocation.timeout, timeout);
    }
}

#[test]
fn test_runner_invocation_all_reasoning_efforts() {
    let temp_dir = std::env::temp_dir();
    let repo_root = temp_dir.as_path();

    let efforts = vec![
        None,
        Some(ReasoningEffort::Low),
        Some(ReasoningEffort::Medium),
        Some(ReasoningEffort::High),
        Some(ReasoningEffort::XHigh),
    ];

    for effort in efforts {
        let invocation = runutil::RunnerInvocation {
            repo_root,
            runner_kind: Runner::Codex,
            bins: ralph::runner::RunnerBinaries {
                codex: "codex",
                opencode: "opencode",
                gemini: "gemini",
                claude: "claude",
            },
            model: Model::Gpt52Codex,
            reasoning_effort: effort,
            prompt: "test",
            timeout: None,
            permission_mode: None,
            revert_on_error: false,
            git_revert_mode: ralph::contracts::GitRevertMode::Ask,
            output_handler: None,
            output_stream: ralph::runner::OutputStream::Terminal,
            revert_prompt: None,
        };
        assert_eq!(invocation.reasoning_effort, effort);
    }
}
