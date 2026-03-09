//! Shared runutil test fixtures.
//!
//! Responsibilities:
//! - Provide temp git repo helpers reused across runutil test modules.
//! - Provide backend doubles and invocation/message builders for runner handling tests.
//!
//! Does NOT handle:
//! - Assertions for specific revert or validation behaviors.
//! - Real subprocess-backed runner execution.
//!
//! Invariants:
//! - Test repos are initialized with deterministic git identity.
//! - Test backends only implement the code paths required by their owning tests.

use crate::commands::run::PhaseType;
use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::runner;
use crate::runutil::{RunnerBackend, RunnerErrorMessages, RunnerInvocation};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

type TestRunnerErrorMessages =
    RunnerErrorMessages<'static, fn(i32) -> String, fn(crate::runner::RunnerError) -> String>;

pub(super) fn init_git_repo(dir: &TempDir) {
    Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(dir.path())
        .output()
        .expect("git init failed");

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir.path())
        .output()
        .expect("git config user.email failed");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir.path())
        .output()
        .expect("git config user.name failed");
}

pub(super) fn commit_file(dir: &TempDir, filename: &str, content: &str, message: &str) {
    let file_path = dir.path().join(filename);
    fs::write(&file_path, content).expect("write file");

    Command::new("git")
        .args(["add", filename])
        .current_dir(dir.path())
        .output()
        .expect("git add failed");

    Command::new("git")
        .args(["commit", "--quiet", "-m", message])
        .current_dir(dir.path())
        .output()
        .expect("git commit failed");
}

pub(super) fn base_invocation<'a>(repo_root: &'a Path) -> RunnerInvocation<'a> {
    RunnerInvocation {
        repo_root,
        runner_kind: Runner::Codex,
        bins: runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "agent",
            kimi: "kimi",
            pi: "pi",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
        prompt: "test prompt",
        timeout: None,
        permission_mode: None,
        revert_on_error: false,
        git_revert_mode: GitRevertMode::Disabled,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        revert_prompt: None,
        phase_type: PhaseType::Implementation,
        session_id: None,
        retry_policy: crate::runutil::RunnerRetryPolicy::default(),
    }
}

pub(super) fn base_messages(log_label: &'static str) -> TestRunnerErrorMessages {
    RunnerErrorMessages {
        log_label,
        interrupted_msg: "interrupted",
        timeout_msg: "timed out",
        terminated_msg: "terminated",
        non_zero_msg: |_| "non-zero".to_string(),
        other_msg: |_| "other".to_string(),
    }
}

pub(super) struct TimeoutBackend {
    pub emitted: String,
}

impl RunnerBackend for TimeoutBackend {
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
        output_handler: Option<runner::OutputHandler>,
        _output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _session_id: Option<String>,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        if let Some(handler) = output_handler {
            (handler)(&self.emitted);
        }
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
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for a timeout-only test backend");
    }
}

pub(super) struct InterruptBackend;

impl RunnerBackend for InterruptBackend {
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
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        Err(runner::RunnerError::Interrupted)
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
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for interrupt test");
    }
}

pub(super) struct NonZeroBackend;

impl RunnerBackend for NonZeroBackend {
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
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        Err(runner::RunnerError::NonZeroExit {
            code: 1,
            stdout: "stdout".into(),
            stderr: "stderr".into(),
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
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for non-zero test");
    }
}

pub(super) struct CaptureBackend {
    pub seen_output_stream: Option<runner::OutputStream>,
}

impl RunnerBackend for CaptureBackend {
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
        output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _session_id: Option<String>,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        self.seen_output_stream = Some(output_stream);
        Err(runner::RunnerError::Interrupted)
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
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for output-stream capture test");
    }
}
