//! Shared runutil test fixtures.
//!
//! Purpose:
//! - Shared runutil test fixtures.
//!
//! Responsibilities:
//! - Provide temp git repo helpers reused across runutil test modules.
//! - Provide backend doubles and invocation/message builders for runner handling tests.
//!
//! Non-scope:
//! - Assertions for specific revert or validation behaviors.
//! - Real subprocess-backed runner execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Test repos are initialized with deterministic git identity.
//! - Test backends only implement the code paths required by their owning tests.

use crate::commands::run::PhaseType;
use crate::contracts::{GitRevertMode, Model, Runner};
use crate::runner;
use crate::runutil::{
    RunnerBackend, RunnerBackendResumeSession, RunnerBackendRunPrompt, RunnerErrorMessages,
    RunnerExecutionContext, RunnerFailureHandling, RunnerInvocation, RunnerRetryState,
    RunnerSettings,
};
use std::fs;
use std::path::Path;
use std::process::Command;
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
        settings: RunnerSettings {
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
            model: Model::Gpt53Codex,
            reasoning_effort: None,
            runner_cli: runner::ResolvedRunnerCliOptions::default(),
            timeout: None,
            permission_mode: None,
            output_handler: None,
            output_stream: runner::OutputStream::Terminal,
        },
        execution: RunnerExecutionContext {
            prompt: "test prompt",
            phase_type: PhaseType::Implementation,
            session_id: None,
        },
        failure: RunnerFailureHandling {
            revert_on_error: false,
            git_revert_mode: GitRevertMode::Disabled,
            revert_prompt: None,
        },
        retry: RunnerRetryState {
            policy: crate::runutil::RunnerRetryPolicy::default(),
        },
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
    fn run_prompt(
        &mut self,
        request: RunnerBackendRunPrompt<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        if let Some(handler) = request.output_handler {
            (handler)(&self.emitted);
        }
        Err(runner::RunnerError::Timeout)
    }

    fn resume_session(
        &mut self,
        _request: RunnerBackendResumeSession<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for a timeout-only test backend");
    }
}

pub(super) struct InterruptBackend;

impl RunnerBackend for InterruptBackend {
    fn run_prompt(
        &mut self,
        _request: RunnerBackendRunPrompt<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        Err(runner::RunnerError::Interrupted)
    }

    fn resume_session(
        &mut self,
        _request: RunnerBackendResumeSession<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for interrupt test");
    }
}

pub(super) struct NonZeroBackend;

impl RunnerBackend for NonZeroBackend {
    fn run_prompt(
        &mut self,
        _request: RunnerBackendRunPrompt<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        Err(runner::RunnerError::NonZeroExit {
            code: 1,
            stdout: "stdout".into(),
            stderr: "stderr".into(),
            session_id: None,
        })
    }

    fn resume_session(
        &mut self,
        _request: RunnerBackendResumeSession<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for non-zero test");
    }
}

pub(super) struct CaptureBackend {
    pub seen_output_stream: Option<runner::OutputStream>,
}

impl RunnerBackend for CaptureBackend {
    fn run_prompt(
        &mut self,
        request: RunnerBackendRunPrompt<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        self.seen_output_stream = Some(request.output_stream);
        Err(runner::RunnerError::Interrupted)
    }

    fn resume_session(
        &mut self,
        _request: RunnerBackendResumeSession<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for output-stream capture test");
    }
}
