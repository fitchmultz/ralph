//! Runner execution backend types and utility hooks.
//!
//! Purpose:
//! - Runner execution backend types and utility hooks.
//!
//! Responsibilities:
//! - Define the invocation/config types for runner execution orchestration.
//! - Provide the real backend implementation that delegates to `crate::runner`.
//! - Host output-capture/logging helpers reused across orchestration paths.
//!
//! Not handled here:
//! - Retry/continue-session policy.
//! - Main error-handling state machine.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Output capture remains bounded.
//! - Callers provide validated runner/model settings.

use anyhow::Result;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::commands::run::PhaseType;
use crate::constants::buffers::{OUTPUT_TAIL_LINE_MAX_CHARS, OUTPUT_TAIL_LINES};
use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::{outpututil, runner};

pub(crate) struct RunnerInvocation<'a> {
    pub settings: RunnerSettings<'a>,
    pub execution: RunnerExecutionContext<'a>,
    pub failure: RunnerFailureHandling,
    pub retry: RunnerRetryState,
}

pub(crate) struct RunnerSettings<'a> {
    pub repo_root: &'a Path,
    pub runner_kind: Runner,
    pub bins: runner::RunnerBinaries<'a>,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: runner::ResolvedRunnerCliOptions,
    pub timeout: Option<Duration>,
    pub permission_mode: Option<ClaudePermissionMode>,
    pub output_handler: Option<runner::OutputHandler>,
    pub output_stream: runner::OutputStream,
}

impl RunnerSettings<'_> {
    pub(super) fn attempt_context<'context>(
        &'context self,
        output_handler: Option<runner::OutputHandler>,
        phase_type: PhaseType,
    ) -> RunnerAttemptContext<'context> {
        RunnerAttemptContext {
            runner_kind: &self.runner_kind,
            repo_root: self.repo_root,
            bins: self.bins,
            model: &self.model,
            reasoning_effort: self.reasoning_effort,
            runner_cli: self.runner_cli,
            timeout: self.timeout,
            permission_mode: self.permission_mode,
            output_handler,
            output_stream: self.output_stream,
            phase_type,
        }
    }
}

pub(crate) struct RunnerExecutionContext<'a> {
    pub prompt: &'a str,
    pub phase_type: PhaseType,
    pub session_id: Option<String>,
}

pub(crate) struct RunnerFailureHandling {
    pub revert_on_error: bool,
    pub git_revert_mode: GitRevertMode,
    pub revert_prompt: Option<super::super::revert::RevertPromptHandler>,
}

#[derive(Clone, Copy)]
pub(crate) struct RunnerRetryState {
    pub policy: super::super::RunnerRetryPolicy,
}

pub(crate) struct RunnerErrorMessages<'a, FNonZero, FOther>
where
    FNonZero: FnMut(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    pub log_label: &'a str,
    pub interrupted_msg: &'a str,
    pub timeout_msg: &'a str,
    pub terminated_msg: &'a str,
    pub non_zero_msg: FNonZero,
    pub other_msg: FOther,
}

pub(super) struct RunnerAttemptContext<'a> {
    pub runner_kind: &'a Runner,
    pub repo_root: &'a Path,
    pub bins: runner::RunnerBinaries<'a>,
    pub model: &'a Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: runner::ResolvedRunnerCliOptions,
    pub timeout: Option<Duration>,
    pub permission_mode: Option<ClaudePermissionMode>,
    pub output_handler: Option<runner::OutputHandler>,
    pub output_stream: runner::OutputStream,
    pub phase_type: PhaseType,
}

impl RunnerAttemptContext<'_> {
    pub(super) fn run_prompt_request<'request>(
        &'request self,
        prompt: &'request str,
        session_id: Option<String>,
    ) -> RunnerBackendRunPrompt<'request> {
        RunnerBackendRunPrompt {
            runner_kind: self.runner_kind.clone(),
            work_dir: self.repo_root,
            bins: self.bins,
            model: self.model.clone(),
            reasoning_effort: self.reasoning_effort,
            runner_cli: self.runner_cli,
            prompt,
            timeout: self.timeout,
            permission_mode: self.permission_mode,
            output_handler: self.output_handler.clone(),
            output_stream: self.output_stream,
            phase_type: self.phase_type,
            session_id,
            plugins: None,
        }
    }

    pub(super) fn resume_session_request<'request>(
        &'request self,
        session_id: &'request str,
        message: &'request str,
    ) -> RunnerBackendResumeSession<'request> {
        RunnerBackendResumeSession {
            runner_kind: self.runner_kind.clone(),
            work_dir: self.repo_root,
            bins: self.bins,
            model: self.model.clone(),
            reasoning_effort: self.reasoning_effort,
            runner_cli: self.runner_cli,
            session_id,
            message,
            permission_mode: self.permission_mode,
            timeout: self.timeout,
            output_handler: self.output_handler.clone(),
            output_stream: self.output_stream,
            phase_type: self.phase_type,
            plugins: None,
        }
    }
}

pub(crate) struct RunnerBackendRunPrompt<'a> {
    pub runner_kind: Runner,
    pub work_dir: &'a Path,
    pub bins: runner::RunnerBinaries<'a>,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: runner::ResolvedRunnerCliOptions,
    pub prompt: &'a str,
    pub timeout: Option<Duration>,
    pub permission_mode: Option<ClaudePermissionMode>,
    pub output_handler: Option<runner::OutputHandler>,
    pub output_stream: runner::OutputStream,
    pub phase_type: PhaseType,
    pub session_id: Option<String>,
    pub plugins: Option<&'a crate::plugins::registry::PluginRegistry>,
}

pub(crate) struct RunnerBackendResumeSession<'a> {
    pub runner_kind: Runner,
    pub work_dir: &'a Path,
    pub bins: runner::RunnerBinaries<'a>,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: runner::ResolvedRunnerCliOptions,
    pub session_id: &'a str,
    pub message: &'a str,
    pub permission_mode: Option<ClaudePermissionMode>,
    pub timeout: Option<Duration>,
    pub output_handler: Option<runner::OutputHandler>,
    pub output_stream: runner::OutputStream,
    pub phase_type: PhaseType,
    pub plugins: Option<&'a crate::plugins::registry::PluginRegistry>,
}

pub(crate) trait RunnerBackend {
    fn run_prompt(
        &mut self,
        request: RunnerBackendRunPrompt<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError>;

    fn resume_session(
        &mut self,
        request: RunnerBackendResumeSession<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError>;
}

pub(super) struct RealRunnerBackend;

impl RunnerBackend for RealRunnerBackend {
    fn run_prompt(
        &mut self,
        request: RunnerBackendRunPrompt<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        let RunnerBackendRunPrompt {
            runner_kind,
            work_dir,
            bins,
            model,
            reasoning_effort,
            runner_cli,
            prompt,
            timeout,
            permission_mode,
            output_handler,
            output_stream,
            phase_type,
            session_id,
            plugins,
        } = request;

        runner::run_prompt(
            runner_kind,
            work_dir,
            bins,
            model,
            reasoning_effort,
            runner_cli,
            prompt,
            timeout,
            permission_mode,
            output_handler,
            output_stream,
            phase_type,
            session_id,
            plugins,
        )
    }

    fn resume_session(
        &mut self,
        request: RunnerBackendResumeSession<'_>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        let RunnerBackendResumeSession {
            runner_kind,
            work_dir,
            bins,
            model,
            reasoning_effort,
            runner_cli,
            session_id,
            message,
            permission_mode,
            timeout,
            output_handler,
            output_stream,
            phase_type,
            plugins,
        } = request;

        runner::resume_session(
            runner_kind,
            work_dir,
            bins,
            model,
            reasoning_effort,
            runner_cli,
            session_id,
            message,
            permission_mode,
            timeout,
            output_handler,
            output_stream,
            phase_type,
            plugins,
        )
    }
}

pub(super) fn wrap_output_handler_with_capture(
    existing: Option<runner::OutputHandler>,
    max_bytes: usize,
) -> (Arc<Mutex<String>>, Option<runner::OutputHandler>) {
    let capture = Arc::new(Mutex::new(String::new()));
    let capture_for_handler = capture.clone();
    let existing_for_handler = existing.clone();

    let handler: runner::OutputHandler = Arc::new(Box::new(move |chunk: &str| {
        fn append_chunk(buf: &mut String, chunk: &str, max_bytes: usize) {
            buf.push_str(chunk);
            if buf.len() > max_bytes {
                let excess = buf.len() - max_bytes;
                buf.drain(..excess);
            }
        }

        match capture_for_handler.lock() {
            Ok(mut buf) => append_chunk(&mut buf, chunk, max_bytes),
            Err(poisoned) => {
                log::warn!("timeout_stdout_capture mutex poisoned; recovering captured output");
                let mut buf = poisoned.into_inner();
                append_chunk(&mut buf, chunk, max_bytes);
            }
        }
        if let Some(existing) = existing_for_handler.as_ref() {
            (existing)(chunk);
        }
    }));

    (capture, Some(handler))
}

pub(super) fn emit_operation(handler: &Option<runner::OutputHandler>, msg: &str) {
    if let Some(handler) = handler.as_ref() {
        (handler)(&format!("RALPH_OPERATION: {}\n", msg));
    }
}

pub(super) fn log_stderr_tail(label: &str, stderr: &str) {
    let tail = outpututil::tail_lines(stderr, OUTPUT_TAIL_LINES, OUTPUT_TAIL_LINE_MAX_CHARS);
    if tail.is_empty() {
        return;
    }

    crate::rerror!("{label} stderr (tail):");
    for line in tail {
        crate::rinfo!("{label}: {line}");
    }
}
