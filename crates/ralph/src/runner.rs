//! Runner orchestration for executing tasks across supported CLIs and parsing outputs.
//!
//! Purpose:
//! - Runner orchestration for executing tasks across supported CLIs and parsing outputs.
//!
//! Responsibilities:
//! - Expose the runner orchestration API (`run_prompt`, `resume_session`) and shared types.
//! - Delegate execution details to `runner/execution/*`.
//! - Re-export cohesive submodules for errors, models, and settings.
//!
//! Non-scope:
//! - Runner subprocess command assembly (see `runner/execution/*`).
//! - Queue persistence or task selection.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Runner output is redacted before display/logging where required.

mod error;
mod execution;
mod invoke;
mod model;
mod plugin_dispatch;
mod settings;

#[cfg(test)]
mod tests;

pub use error::RunnerError;
pub(crate) use error::{
    RetryableReason, RunnerFailureClass, runner_execution_error, runner_execution_error_with_source,
};

pub(crate) use execution::{
    BuiltInRunnerPlugin, CtrlCState, ResolvedRunnerCliOptions, RunnerPlugin, ctrlc_state,
};

pub(crate) use model::{
    default_model_for_runner, parse_model, parse_reasoning_effort, resolve_model_for_runner,
    validate_model_for_runner,
};

pub(crate) use settings::{
    AgentSettings, PhaseSettingsMatrix, ResolvedPhaseSettings, resolve_agent_settings,
    resolve_phase_settings_matrix,
};

// Prevent clippy --fix from removing this re-export (used by commands/run/tests.rs)
#[allow(unused)]
const _: () = {
    fn _use_resolved_phase_settings(_: ResolvedPhaseSettings) {}
};

use crate::commands::run::PhaseType;
use crate::contracts::{ClaudePermissionMode, Model, ReasoningEffort, Runner};
use crate::plugins::registry::PluginRegistry;
use crate::redaction::redact_text;
use anyhow::Result;
use std::fmt;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;

/// Callback type for streaming runner output to consumers (e.g., the macOS app).
/// Called with each chunk of output as it's received from the runner process.
pub type OutputHandler = Arc<Box<dyn Fn(&str) + Send + Sync>>;

/// Controls whether runner output is streamed directly to the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    /// Stream runner output to stdout/stderr as well as any output handler.
    Terminal,
    /// Suppress direct terminal output and only deliver output to the handler.
    HandlerOnly,
}

impl OutputStream {
    /// Returns true when output should be streamed to stdout/stderr.
    pub fn streams_to_terminal(self) -> bool {
        matches!(self, OutputStream::Terminal)
    }
}

pub(crate) struct RunnerOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub session_id: Option<String>,
}

impl fmt::Display for RunnerOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "status: {}\nstdout: {}\nstderr: {}",
            self.status,
            redact_text(&self.stdout),
            redact_text(&self.stderr)
        )
    }
}

impl fmt::Debug for RunnerOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunnerOutput")
            .field("status", &self.status)
            .field("stdout", &redact_text(&self.stdout))
            .field("stderr", &redact_text(&self.stderr))
            .field("session_id", &self.session_id.as_deref())
            .finish()
    }
}

#[derive(Clone, Copy)]
pub struct RunnerBinaries<'a> {
    pub codex: &'a str,
    pub opencode: &'a str,
    pub gemini: &'a str,
    pub claude: &'a str,
    pub cursor: &'a str,
    pub kimi: &'a str,
    pub pi: &'a str,
}

pub(crate) fn resolve_binaries(agent: &crate::contracts::AgentConfig) -> RunnerBinaries<'_> {
    let codex = agent.codex_bin.as_deref().unwrap_or("codex");
    let opencode = agent.opencode_bin.as_deref().unwrap_or("opencode");
    let gemini = agent.gemini_bin.as_deref().unwrap_or("gemini");
    let claude = agent.claude_bin.as_deref().unwrap_or("claude");
    let cursor = agent.cursor_bin.as_deref().unwrap_or("agent");
    let kimi = agent.kimi_bin.as_deref().unwrap_or("kimi");
    let pi = agent.pi_bin.as_deref().unwrap_or("pi");
    RunnerBinaries {
        codex,
        opencode,
        gemini,
        claude,
        cursor,
        kimi,
        pi,
    }
}

pub(crate) fn extract_final_assistant_response(stdout: &str) -> Option<String> {
    execution::extract_final_assistant_response(stdout)
}

fn runner_label(runner: Runner) -> String {
    runner.id().to_string()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_prompt(
    runner: Runner,
    work_dir: &Path,
    bins: RunnerBinaries<'_>,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: execution::ResolvedRunnerCliOptions,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    phase_type: PhaseType,
    session_id: Option<String>,
    plugins: Option<&PluginRegistry>,
) -> Result<RunnerOutput, RunnerError> {
    invoke::dispatch(
        invoke::RunnerDispatchContext {
            runner,
            work_dir,
            bins,
            model,
            reasoning_effort,
            runner_cli,
            timeout,
            permission_mode,
            output_handler,
            output_stream,
            phase_type,
            plugins,
        },
        invoke::RunnerInvocation::Prompt { prompt, session_id },
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resume_session(
    runner: Runner,
    work_dir: &Path,
    bins: RunnerBinaries<'_>,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: execution::ResolvedRunnerCliOptions,
    session_id: &str,
    message: &str,
    permission_mode: Option<ClaudePermissionMode>,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    phase_type: PhaseType,
    plugins: Option<&PluginRegistry>,
) -> Result<RunnerOutput, RunnerError> {
    invoke::dispatch(
        invoke::RunnerDispatchContext {
            runner,
            work_dir,
            bins,
            model,
            reasoning_effort,
            runner_cli,
            timeout,
            permission_mode,
            output_handler,
            output_stream,
            phase_type,
            plugins,
        },
        invoke::RunnerInvocation::Resume {
            session_id,
            message,
        },
    )
}
