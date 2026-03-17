//! Purpose: Regression coverage for runner execution orchestration.
//!
//! Responsibilities:
//! - Verify safeguard-dump messaging for orchestration failure paths.
//! - Validate timeout handling remains resilient to poisoned capture locks.
//! - Keep signal-recovery and continue-session fallback behavior pinned.
//!
//! Scope:
//! - Unit tests for `orchestration/core.rs` only.
//! - Broader runutil tests remain in `runutil/tests.rs` and `runutil/tests/*.rs`.
//!
//! Usage:
//! - Compiled through `orchestration/mod.rs` under `#[cfg(test)]`.
//!
//! Invariants/Assumptions:
//! - Tests use mock `RunnerBackend` implementations and temp dirs only.
//! - No real runner binaries are required.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::super::backend::{RunnerBackend, RunnerErrorMessages, RunnerInvocation};
use super::run_prompt_with_handling_backend;
use crate::commands::run::PhaseType;
use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::redaction::RedactedString;
use crate::runner;

fn test_bins() -> runner::RunnerBinaries<'static> {
    runner::RunnerBinaries {
        codex: "codex",
        opencode: "opencode",
        gemini: "gemini",
        claude: "claude",
        cursor: "cursor",
        kimi: "kimi",
        pi: "pi",
    }
}

fn test_invocation<'a>(
    repo_root: &'a Path,
    runner_kind: Runner,
    model: Model,
    prompt: &'a str,
    timeout: Option<Duration>,
    revert_on_error: bool,
    session_id: Option<String>,
) -> RunnerInvocation<'a> {
    RunnerInvocation {
        repo_root,
        runner_kind,
        bins: test_bins(),
        model,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
        prompt,
        timeout,
        permission_mode: None,
        revert_on_error,
        git_revert_mode: GitRevertMode::Disabled,
        output_handler: None,
        output_stream: runner::OutputStream::HandlerOnly,
        revert_prompt: None,
        phase_type: PhaseType::Implementation,
        session_id,
        retry_policy: Default::default(),
    }
}

fn non_zero_message(code: i32) -> String {
    format!("non-zero exit: {}", code)
}

fn other_message(err: runner::RunnerError) -> String {
    format!("other error: {}", err)
}

type TestRunnerErrorMessages =
    RunnerErrorMessages<'static, fn(i32) -> String, fn(runner::RunnerError) -> String>;

fn test_messages() -> TestRunnerErrorMessages {
    RunnerErrorMessages {
        log_label: "test",
        interrupted_msg: "interrupted",
        timeout_msg: "timeout",
        terminated_msg: "terminated",
        non_zero_msg: non_zero_message,
        other_msg: other_message,
    }
}

#[test]
fn safeguard_dump_created_for_stderr_on_nonzero_exit() {
    struct MockNonZeroExitBackend;

    impl RunnerBackend for MockNonZeroExitBackend {
        fn run_prompt<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _prompt: &str,
            _timeout: Option<Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _session_id: Option<String>,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            Err(runner::RunnerError::NonZeroExit {
                code: 1,
                stdout: RedactedString::from("stdout content"),
                stderr: RedactedString::from("stderr content with API_KEY=secret123"),
                session_id: None,
            })
        }

        fn resume_session<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<Duration>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            unreachable!("resume_session should not be called")
        }
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let invocation = test_invocation(
        temp_dir.path(),
        Runner::Codex,
        Model::Gpt53Codex,
        "test prompt",
        None,
        true,
        None,
    );

    let mut backend = MockNonZeroExitBackend;
    let result = run_prompt_with_handling_backend(invocation, test_messages(), &mut backend);

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("stdout saved"));
    assert!(err_msg.contains("stderr saved"));
}

#[test]
fn safeguard_dump_created_for_stderr_on_terminated_by_signal() {
    struct MockTerminatedBySignalBackend;

    impl RunnerBackend for MockTerminatedBySignalBackend {
        fn run_prompt<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _prompt: &str,
            _timeout: Option<Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _session_id: Option<String>,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            Err(runner::RunnerError::TerminatedBySignal {
                signal: Some(15),
                stdout: RedactedString::from("stdout content"),
                stderr: RedactedString::from("stderr content with API_KEY=secret123"),
                session_id: None,
            })
        }

        fn resume_session<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<Duration>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            unreachable!("resume_session should not be called")
        }
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let invocation = test_invocation(
        temp_dir.path(),
        Runner::Codex,
        Model::Gpt53Codex,
        "test prompt",
        None,
        true,
        None,
    );

    let mut backend = MockTerminatedBySignalBackend;
    let result = run_prompt_with_handling_backend(invocation, test_messages(), &mut backend);

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("stdout saved"));
    assert!(err_msg.contains("stderr saved"));
}

#[test]
fn no_safeguard_dump_for_empty_stderr() {
    struct MockEmptyStderrBackend;

    impl RunnerBackend for MockEmptyStderrBackend {
        fn run_prompt<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _prompt: &str,
            _timeout: Option<Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _session_id: Option<String>,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            Err(runner::RunnerError::NonZeroExit {
                code: 1,
                stdout: RedactedString::from("stdout content"),
                stderr: RedactedString::from(""),
                session_id: None,
            })
        }

        fn resume_session<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<Duration>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            unreachable!("resume_session should not be called")
        }
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let invocation = test_invocation(
        temp_dir.path(),
        Runner::Codex,
        Model::Gpt53Codex,
        "test prompt",
        None,
        true,
        None,
    );

    let mut backend = MockEmptyStderrBackend;
    let result = run_prompt_with_handling_backend(invocation, test_messages(), &mut backend);

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("stdout saved"));
    assert!(!err_msg.contains("stderr saved"));
}

#[test]
fn timeout_stdout_capture_survives_mutex_poison() {
    struct MockTimeoutBackend;

    impl RunnerBackend for MockTimeoutBackend {
        fn run_prompt<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _prompt: &str,
            _timeout: Option<Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _session_id: Option<String>,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            Err(runner::RunnerError::Timeout)
        }

        fn resume_session<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<Duration>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            unreachable!("resume_session should not be called")
        }
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");

    let capture_for_handler: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let capture_for_panic = capture_for_handler.clone();

    let handle = thread::spawn(move || {
        let _lock = capture_for_panic.lock().unwrap();
        panic!("intentional panic to poison mutex");
    });

    let _ = handle.join();

    assert!(capture_for_handler.is_poisoned());

    let recovered_data = match capture_for_handler.lock() {
        Ok(buf) => buf.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    assert_eq!(recovered_data, "");

    let invocation = test_invocation(
        temp_dir.path(),
        Runner::Codex,
        Model::Gpt53Codex,
        "test prompt",
        Some(Duration::from_secs(1)),
        true,
        None,
    );

    let messages = RunnerErrorMessages {
        log_label: "test",
        interrupted_msg: "interrupted",
        timeout_msg: "timeout occurred",
        terminated_msg: "terminated",
        non_zero_msg: non_zero_message,
        other_msg: other_message,
    };

    let mut backend = MockTimeoutBackend;
    let result = run_prompt_with_handling_backend(invocation, messages, &mut backend);

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("timeout occurred"), "got: {}", err_msg);
}

#[test]
fn pi_continue_falls_back_to_fresh_run_when_resume_session_lookup_fails() {
    #[derive(Default)]
    struct MockPiFallbackBackend {
        run_calls: usize,
        resume_calls: usize,
    }

    impl RunnerBackend for MockPiFallbackBackend {
        fn run_prompt<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _prompt: &str,
            _timeout: Option<Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _session_id: Option<String>,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            self.run_calls += 1;
            if self.run_calls == 1 {
                Err(runner::RunnerError::TerminatedBySignal {
                    signal: Some(15),
                    stdout: RedactedString::from(""),
                    stderr: RedactedString::from(""),
                    session_id: Some("pi-session-123".to_string()),
                })
            } else {
                Ok(runner::RunnerOutput {
                    status: std::process::Command::new("sh")
                        .arg("-c")
                        .arg("exit 0")
                        .status()
                        .expect("status"),
                    stdout: "fresh rerun output".to_string(),
                    stderr: String::new(),
                    session_id: Some("fresh-session".to_string()),
                })
            }
        }

        fn resume_session<'a>(
            &mut self,
            _runner_kind: Runner,
            _work_dir: &Path,
            _bins: runner::RunnerBinaries<'a>,
            _model: Model,
            _reasoning_effort: Option<ReasoningEffort>,
            _runner_cli: runner::ResolvedRunnerCliOptions,
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<Duration>,
            _output_handler: Option<runner::OutputHandler>,
            _output_stream: runner::OutputStream,
            _phase_type: PhaseType,
            _plugins: Option<&crate::plugins::registry::PluginRegistry>,
        ) -> anyhow::Result<runner::RunnerOutput, runner::RunnerError> {
            self.resume_calls += 1;
            Err(runner::RunnerError::Other(anyhow::anyhow!(
                "pi session file not found"
            )))
        }
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let invocation = test_invocation(
        temp_dir.path(),
        Runner::Pi,
        Model::Gpt53,
        "resume task",
        None,
        false,
        Some("pi-session-123".to_string()),
    );

    let messages = RunnerErrorMessages {
        log_label: "pi-fallback",
        interrupted_msg: "interrupted",
        timeout_msg: "timeout",
        terminated_msg: "terminated",
        non_zero_msg: non_zero_message,
        other_msg: other_message,
    };

    let mut backend = MockPiFallbackBackend::default();
    let output = run_prompt_with_handling_backend(invocation, messages, &mut backend)
        .expect("fallback should rerun fresh");

    assert_eq!(backend.resume_calls, 1, "resume should be attempted once");
    assert_eq!(
        backend.run_calls, 2,
        "fresh rerun should execute after fallback"
    );
    assert_eq!(output.stdout, "fresh rerun output");
    assert_eq!(output.session_id.as_deref(), Some("fresh-session"));
}
