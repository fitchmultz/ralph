//! Unit tests for `crate::runutil` submodules.
//!
//! Responsibilities:
//! - Validate stderr tail redaction assumptions used by execution logging.
//! - Validate safeguard dump behavior for runner failures in `run_prompt_with_handling_backend`.
//!
//! Not handled here:
//! - Integration with real runner binaries.
//!
//! Invariants/assumptions:
//! - Tests may use mock `RunnerBackend` implementations and temp dirs.

use super::*;
use crate::constants::buffers::{OUTPUT_TAIL_LINE_MAX_CHARS, OUTPUT_TAIL_LINES};
use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::runutil::execution::{RunnerBackend, run_prompt_with_handling_backend};

#[test]
fn log_stderr_tail_redacts_api_keys_via_redact_text() {
    let stderr = "Error occurred\nAPI_KEY=secret12345\nMore output";
    let redacted = crate::redaction::redact_text(stderr);

    assert!(
        !redacted.contains("secret12345"),
        "API key should be redacted"
    );
    assert!(redacted.contains("[REDACTED]"), "Should contain [REDACTED]");
}

#[test]
fn log_stderr_tail_redacts_bearer_tokens_via_redact_text() {
    let stderr = "Authorization: Bearer abcdef123456789\nDone";
    let redacted = crate::redaction::redact_text(stderr);

    assert!(
        !redacted.contains("abcdef123456789"),
        "Bearer token should be redacted"
    );
    assert!(
        redacted.contains("Bearer [REDACTED]"),
        "Should show Bearer [REDACTED]"
    );
}

#[test]
fn log_stderr_tail_handles_empty_stderr() {
    let tail = crate::outpututil::tail_lines("", OUTPUT_TAIL_LINES, OUTPUT_TAIL_LINE_MAX_CHARS);
    assert!(tail.is_empty());
}

#[test]
fn log_stderr_tail_presents_normal_content_via_tail_lines() {
    let stderr = "Normal error message\nAnother line";
    let tail = crate::outpututil::tail_lines(stderr, OUTPUT_TAIL_LINES, OUTPUT_TAIL_LINE_MAX_CHARS);

    assert_eq!(tail.len(), 2);
    assert_eq!(tail[0], "Normal error message");
    assert_eq!(tail[1], "Another line");
}

#[test]
fn log_stderr_tail_uses_rinfo_rerror_macros() {
    let input = "token=secret123";
    let redacted = crate::redaction::redact_text(input);
    assert!(!redacted.contains("secret123"));
    assert!(redacted.contains("[REDACTED]"));
}

#[test]
fn safeguard_dump_created_for_stderr_on_nonzero_exit() {
    use crate::redaction::RedactedString;

    struct MockNonZeroExitBackend;
    impl RunnerBackend for MockNonZeroExitBackend {
        fn run_prompt<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &std::path::Path,
            _bins: crate::runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: crate::runner::ResolvedRunnerCliOptions,
            _prompt: &str,
            _timeout: Option<std::time::Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            _output_handler: Option<crate::runner::OutputHandler>,
            _output_stream: crate::runner::OutputStream,
            _phase_type: crate::commands::run::PhaseType,
            _session_id: Option<String>,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<crate::runner::RunnerOutput, crate::runner::RunnerError> {
            Err(crate::runner::RunnerError::NonZeroExit {
                code: 1,
                stdout: RedactedString::from("stdout content"),
                stderr: RedactedString::from("stderr content with API_KEY=secret123"),
                session_id: None,
            })
        }

        fn resume_session<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &std::path::Path,
            _bins: crate::runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: crate::runner::ResolvedRunnerCliOptions,
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<std::time::Duration>,
            _output_handler: Option<crate::runner::OutputHandler>,
            _output_stream: crate::runner::OutputStream,
            _phase_type: crate::commands::run::PhaseType,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<crate::runner::RunnerOutput, crate::runner::RunnerError> {
            unreachable!("resume_session should not be called")
        }
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let invocation = RunnerInvocation {
        repo_root: temp_dir.path(),
        runner_kind: Runner::Codex,
        bins: crate::runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "cursor",
            kimi: "kimi",
            pi: "pi",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        prompt: "test prompt",
        timeout: None,
        permission_mode: None,
        revert_on_error: true,
        git_revert_mode: GitRevertMode::Disabled,
        output_handler: None,
        output_stream: crate::runner::OutputStream::HandlerOnly,
        revert_prompt: None,
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: None,
    };

    let messages = RunnerErrorMessages {
        log_label: "test",
        interrupted_msg: "interrupted",
        timeout_msg: "timeout",
        terminated_msg: "terminated",
        non_zero_msg: |code| format!("non-zero exit: {}", code),
        other_msg: |err| format!("other error: {}", err),
    };

    let mut backend = MockNonZeroExitBackend;
    let result = run_prompt_with_handling_backend(invocation, messages, &mut backend);

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("stdout saved"),
        "Error should mention stdout dump path"
    );
    assert!(
        err_msg.contains("stderr saved"),
        "Error should mention stderr dump path"
    );
}

#[test]
fn safeguard_dump_created_for_stderr_on_terminated_by_signal() {
    use crate::redaction::RedactedString;

    struct MockTerminatedBySignalBackend;
    impl RunnerBackend for MockTerminatedBySignalBackend {
        fn run_prompt<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &std::path::Path,
            _bins: crate::runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: crate::runner::ResolvedRunnerCliOptions,
            _prompt: &str,
            _timeout: Option<std::time::Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            _output_handler: Option<crate::runner::OutputHandler>,
            _output_stream: crate::runner::OutputStream,
            _phase_type: crate::commands::run::PhaseType,
            _session_id: Option<String>,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<crate::runner::RunnerOutput, crate::runner::RunnerError> {
            Err(crate::runner::RunnerError::TerminatedBySignal {
                stdout: RedactedString::from("stdout content"),
                stderr: RedactedString::from("stderr content with API_KEY=secret123"),
                session_id: None,
            })
        }

        fn resume_session<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &std::path::Path,
            _bins: crate::runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: crate::runner::ResolvedRunnerCliOptions,
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<std::time::Duration>,
            _output_handler: Option<crate::runner::OutputHandler>,
            _output_stream: crate::runner::OutputStream,
            _phase_type: crate::commands::run::PhaseType,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<crate::runner::RunnerOutput, crate::runner::RunnerError> {
            unreachable!("resume_session should not be called")
        }
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let invocation = RunnerInvocation {
        repo_root: temp_dir.path(),
        runner_kind: Runner::Codex,
        bins: crate::runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "cursor",
            kimi: "kimi",
            pi: "pi",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        prompt: "test prompt",
        timeout: None,
        permission_mode: None,
        revert_on_error: true,
        git_revert_mode: GitRevertMode::Disabled,
        output_handler: None,
        output_stream: crate::runner::OutputStream::HandlerOnly,
        revert_prompt: None,
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: None,
    };

    let messages = RunnerErrorMessages {
        log_label: "test",
        interrupted_msg: "interrupted",
        timeout_msg: "timeout",
        terminated_msg: "terminated",
        non_zero_msg: |code| format!("non-zero exit: {}", code),
        other_msg: |err| format!("other error: {}", err),
    };

    let mut backend = MockTerminatedBySignalBackend;
    let result = run_prompt_with_handling_backend(invocation, messages, &mut backend);

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("stdout saved"),
        "Error should mention stdout dump path"
    );
    assert!(
        err_msg.contains("stderr saved"),
        "Error should mention stderr dump path"
    );
}

#[test]
fn safeguard_dump_redacts_secrets_in_stderr() {
    use crate::redaction::RedactedString;

    let stderr_content = "Error: API_KEY=sk-abc123xyz789\nAuthorization: Bearer secret_token";
    let stdout = RedactedString::from("stdout content");
    let stderr = RedactedString::from(stderr_content);

    let stdout_str = stdout.to_string();
    let stderr_str = stderr.to_string();

    assert!(
        !stderr_str.contains("sk-abc123xyz789"),
        "API key should be redacted in stderr: {}",
        stderr_str
    );
    assert!(
        !stderr_str.contains("secret_token"),
        "Bearer token should be redacted in stderr: {}",
        stderr_str
    );
    assert!(
        stderr_str.contains("[REDACTED]"),
        "Redacted marker should be present: {}",
        stderr_str
    );

    assert!(
        stdout_str.contains("stdout content"),
        "Normal stdout should be preserved: {}",
        stdout_str
    );
}

#[test]
fn no_safeguard_dump_for_empty_stderr() {
    use crate::redaction::RedactedString;

    struct MockEmptyStderrBackend;
    impl RunnerBackend for MockEmptyStderrBackend {
        fn run_prompt<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &std::path::Path,
            _bins: crate::runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: crate::runner::ResolvedRunnerCliOptions,
            _prompt: &str,
            _timeout: Option<std::time::Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            _output_handler: Option<crate::runner::OutputHandler>,
            _output_stream: crate::runner::OutputStream,
            _phase_type: crate::commands::run::PhaseType,
            _session_id: Option<String>,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<crate::runner::RunnerOutput, crate::runner::RunnerError> {
            Err(crate::runner::RunnerError::NonZeroExit {
                code: 1,
                stdout: RedactedString::from("stdout content"),
                stderr: RedactedString::from(""),
                session_id: None,
            })
        }

        fn resume_session<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &std::path::Path,
            _bins: crate::runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: crate::runner::ResolvedRunnerCliOptions,
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<std::time::Duration>,
            _output_handler: Option<crate::runner::OutputHandler>,
            _output_stream: crate::runner::OutputStream,
            _phase_type: crate::commands::run::PhaseType,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<crate::runner::RunnerOutput, crate::runner::RunnerError> {
            unreachable!("resume_session should not be called")
        }
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let invocation = RunnerInvocation {
        repo_root: temp_dir.path(),
        runner_kind: Runner::Codex,
        bins: crate::runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "cursor",
            kimi: "kimi",
            pi: "pi",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        prompt: "test prompt",
        timeout: None,
        permission_mode: None,
        revert_on_error: true,
        git_revert_mode: GitRevertMode::Disabled,
        output_handler: None,
        output_stream: crate::runner::OutputStream::HandlerOnly,
        revert_prompt: None,
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: None,
    };

    let messages = RunnerErrorMessages {
        log_label: "test",
        interrupted_msg: "interrupted",
        timeout_msg: "timeout",
        terminated_msg: "terminated",
        non_zero_msg: |code| format!("non-zero exit: {}", code),
        other_msg: |err| format!("other error: {}", err),
    };

    let mut backend = MockEmptyStderrBackend;
    let result = run_prompt_with_handling_backend(invocation, messages, &mut backend);

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("stdout saved"),
        "Error should mention stdout dump path"
    );
    assert!(
        !err_msg.contains("stderr saved"),
        "Error should NOT mention stderr dump path when stderr is empty"
    );
}
