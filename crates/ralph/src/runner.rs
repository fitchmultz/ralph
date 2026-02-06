//! Runner orchestration for executing tasks across supported CLIs and parsing outputs.
//!
//! Responsibilities:
//! - Expose the runner orchestration API (`run_prompt`, `resume_session`) and shared types.
//! - Delegate execution details to `runner/execution/*`.
//! - Re-export cohesive submodules for errors, models, and settings.
//!
//! Does not handle:
//! - Runner subprocess command assembly (see `runner/execution/*`).
//! - Queue persistence or task selection.
//!
//! Assumptions/invariants:
//! - Runner output is redacted before display/logging where required.

mod error;
mod execution;
mod model;
mod settings;

pub use error::RunnerError;
pub(crate) use error::{runner_execution_error, runner_execution_error_with_source};

pub(crate) use execution::{ResolvedRunnerCliOptions, ctrlc_state};

pub(crate) use model::{
    parse_model, parse_reasoning_effort, resolve_model_for_runner, validate_model_for_runner,
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
use anyhow::{Result, anyhow};
use std::fmt;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;

/// Callback type for streaming runner output to consumers (e.g., TUI).
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

fn runner_requires_session_id(_runner: &Runner) -> bool {
    // All runners require session_id for proper resume.
    // Kimi previously allowed empty session_id with --continue flag,
    // but now properly passes --session <id> for resumption.
    true
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
    // Handle plugin runners
    if let Runner::Plugin(plugin_id) = &runner {
        return run_plugin_prompt(
            plugin_id,
            work_dir,
            runner_cli,
            model,
            prompt,
            timeout,
            output_handler,
            output_stream,
            session_id,
            plugins,
        );
    }

    let bin = match runner {
        Runner::Codex => bins.codex,
        Runner::Opencode => bins.opencode,
        Runner::Gemini => bins.gemini,
        Runner::Cursor => bins.cursor,
        Runner::Claude => bins.claude,
        Runner::Kimi => bins.kimi,
        Runner::Pi => bins.pi,
        Runner::Plugin(_) => unreachable!(),
    };
    validate_model_for_runner(&runner, &model).map_err(|err| {
        RunnerError::Other(anyhow!(
            "Runner configuration error (operation=run_prompt, runner={}, bin={}): {}",
            runner_label(runner.clone()),
            bin,
            err
        ))
    })?;
    let output = match runner {
        Runner::Codex => {
            let executor = execution::PluginExecutor::new();
            executor.run(
                Runner::Codex,
                work_dir,
                bins.codex,
                model,
                reasoning_effort,
                runner_cli,
                prompt,
                timeout,
                None, // permission_mode
                output_handler.clone(),
                output_stream,
                phase_type,
                session_id.clone(),
                plugins,
            )?
        }
        Runner::Opencode => {
            let executor = execution::PluginExecutor::new();
            executor.run(
                Runner::Opencode,
                work_dir,
                bins.opencode,
                model,
                None, // reasoning_effort
                runner_cli,
                prompt,
                timeout,
                None, // permission_mode
                output_handler.clone(),
                output_stream,
                phase_type,
                session_id.clone(),
                plugins,
            )?
        }
        Runner::Gemini => {
            let executor = execution::PluginExecutor::new();
            executor.run(
                Runner::Gemini,
                work_dir,
                bins.gemini,
                model,
                None, // reasoning_effort
                runner_cli,
                prompt,
                timeout,
                None, // permission_mode
                output_handler.clone(),
                output_stream,
                phase_type,
                session_id.clone(),
                plugins,
            )?
        }
        Runner::Cursor => execution::run_cursor(
            work_dir,
            bins.cursor,
            runner_cli,
            model,
            prompt,
            timeout,
            output_handler.clone(),
            output_stream,
            phase_type,
        )?,
        Runner::Claude => {
            let executor = execution::PluginExecutor::new();
            executor.run(
                Runner::Claude,
                work_dir,
                bins.claude,
                model,
                reasoning_effort,
                runner_cli,
                prompt,
                timeout,
                runner_cli.effective_claude_permission_mode(permission_mode),
                output_handler.clone(),
                output_stream,
                phase_type,
                session_id.clone(),
                plugins,
            )?
        }
        Runner::Kimi => {
            // Use new plugin-based execution
            let executor = execution::PluginExecutor::new();
            executor.run(
                Runner::Kimi,
                work_dir,
                bins.kimi,
                model,
                None, // reasoning_effort
                runner_cli,
                prompt,
                timeout,
                None, // permission_mode
                output_handler.clone(),
                output_stream,
                phase_type,
                session_id.clone(),
                plugins,
            )?
        }
        Runner::Pi => execution::run_pi(
            work_dir,
            bins.pi,
            runner_cli,
            model,
            prompt,
            timeout,
            output_handler.clone(),
            output_stream,
        )?,
        Runner::Plugin(_) => unreachable!(),
    };

    if !output.status.success() {
        if let Some(code) = output.status.code() {
            return Err(RunnerError::NonZeroExit {
                code,
                stdout: output.stdout.into(),
                stderr: output.stderr.into(),
                session_id: output.session_id.clone(),
            });
        } else {
            return Err(RunnerError::TerminatedBySignal {
                stdout: output.stdout.into(),
                stderr: output.stderr.into(),
                session_id: output.session_id.clone(),
            });
        }
    }

    Ok(output)
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
    // Handle plugin runners
    if let Runner::Plugin(plugin_id) = &runner {
        return run_plugin_resume(
            plugin_id,
            work_dir,
            runner_cli,
            model,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
            plugins,
        );
    }

    let bin = match runner {
        Runner::Codex => bins.codex,
        Runner::Opencode => bins.opencode,
        Runner::Gemini => bins.gemini,
        Runner::Cursor => bins.cursor,
        Runner::Claude => bins.claude,
        Runner::Kimi => bins.kimi,
        Runner::Pi => bins.pi,
        Runner::Plugin(_) => unreachable!(),
    };
    validate_model_for_runner(&runner, &model).map_err(|err| {
        RunnerError::Other(anyhow!(
            "Runner configuration error (operation=resume_session, runner={}, bin={}): {}",
            runner_label(runner.clone()),
            bin,
            err
        ))
    })?;
    let session_id = session_id.trim();
    if runner_requires_session_id(&runner) && session_id.is_empty() {
        return Err(RunnerError::Other(anyhow!(
            "Runner input error (operation=resume_session, runner={}, bin={}): session_id is required (non-empty). Example: --resume <SESSION_ID>.",
            runner_label(runner.clone()),
            bin
        )));
    }
    let message = message.trim();
    if message.is_empty() {
        return Err(RunnerError::Other(anyhow!(
            "Runner input error (operation=resume_session, runner={}, bin={}): message is required (non-empty).",
            runner_label(runner.clone()),
            bin
        )));
    }

    let output = match runner {
        Runner::Codex => {
            let executor = execution::PluginExecutor::new();
            executor.resume(
                Runner::Codex,
                work_dir,
                bins.codex,
                model,
                reasoning_effort,
                runner_cli,
                session_id,
                message,
                timeout,
                None, // permission_mode
                output_handler,
                output_stream,
                phase_type,
                plugins,
            )
        }
        Runner::Opencode => {
            let executor = execution::PluginExecutor::new();
            executor.resume(
                Runner::Opencode,
                work_dir,
                bins.opencode,
                model,
                None, // reasoning_effort
                runner_cli,
                session_id,
                message,
                timeout,
                None, // permission_mode
                output_handler,
                output_stream,
                phase_type,
                plugins,
            )
        }
        Runner::Gemini => {
            let executor = execution::PluginExecutor::new();
            executor.resume(
                Runner::Gemini,
                work_dir,
                bins.gemini,
                model,
                None, // reasoning_effort
                runner_cli,
                session_id,
                message,
                timeout,
                None, // permission_mode
                output_handler,
                output_stream,
                phase_type,
                plugins,
            )
        }
        Runner::Cursor => execution::run_cursor_resume(
            work_dir,
            bins.cursor,
            runner_cli,
            model,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
            phase_type,
        ),
        Runner::Claude => {
            let executor = execution::PluginExecutor::new();
            executor.resume(
                Runner::Claude,
                work_dir,
                bins.claude,
                model,
                reasoning_effort,
                runner_cli,
                session_id,
                message,
                timeout,
                runner_cli.effective_claude_permission_mode(permission_mode),
                output_handler,
                output_stream,
                phase_type,
                plugins,
            )
        }
        Runner::Kimi => {
            // Use new plugin-based execution
            let executor = execution::PluginExecutor::new();
            executor.resume(
                Runner::Kimi,
                work_dir,
                bins.kimi,
                model,
                None, // reasoning_effort
                runner_cli,
                session_id,
                message,
                timeout,
                None, // permission_mode
                output_handler,
                output_stream,
                phase_type,
                plugins,
            )
        }
        Runner::Pi => execution::run_pi_resume(
            work_dir,
            bins.pi,
            runner_cli,
            model,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
        ),
        Runner::Plugin(_) => unreachable!(),
    }?;

    if !output.status.success() {
        if let Some(code) = output.status.code() {
            return Err(RunnerError::NonZeroExit {
                code,
                stdout: output.stdout.into(),
                stderr: output.stderr.into(),
                session_id: output.session_id.clone(),
            });
        } else {
            return Err(RunnerError::TerminatedBySignal {
                stdout: output.stdout.into(),
                stderr: output.stderr.into(),
                session_id: output.session_id.clone(),
            });
        }
    }

    Ok(output)
}

// Helper function to run plugin prompts
#[allow(clippy::too_many_arguments)]
fn run_plugin_prompt(
    plugin_id: &str,
    work_dir: &Path,
    runner_cli: execution::ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    session_id: Option<String>,
    plugins: Option<&PluginRegistry>,
) -> Result<RunnerOutput, RunnerError> {
    let registry = plugins.ok_or_else(|| {
        RunnerError::Other(anyhow!(
            "Plugin registry unavailable for plugin runner: {}",
            plugin_id
        ))
    })?;

    if !registry.is_enabled(plugin_id) {
        return Err(RunnerError::Other(anyhow!(
            "Plugin runner is disabled: {}. Enable it under config.plugins.plugins.{}.enabled=true",
            plugin_id,
            plugin_id
        )));
    }

    let bin_path = registry
        .resolve_runner_bin(plugin_id)
        .map_err(RunnerError::Other)?;
    let bin = bin_path.to_string_lossy().to_string();

    let plugin_cfg = registry
        .plugin_config_blob(plugin_id)
        .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string()));

    execution::run_plugin_runner(
        work_dir,
        &bin,
        plugin_id,
        runner_cli,
        model,
        prompt,
        timeout,
        output_handler,
        output_stream,
        session_id.as_deref(),
        plugin_cfg,
    )
}

// Helper function to resume plugin sessions
#[allow(clippy::too_many_arguments)]
fn run_plugin_resume(
    plugin_id: &str,
    work_dir: &Path,
    runner_cli: execution::ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    plugins: Option<&PluginRegistry>,
) -> Result<RunnerOutput, RunnerError> {
    let registry = plugins.ok_or_else(|| {
        RunnerError::Other(anyhow!(
            "Plugin registry unavailable for plugin runner: {}",
            plugin_id
        ))
    })?;

    if !registry.is_enabled(plugin_id) {
        return Err(RunnerError::Other(anyhow!(
            "Plugin runner is disabled: {}. Enable it under config.plugins.plugins.{}.enabled=true",
            plugin_id,
            plugin_id
        )));
    }

    let bin_path = registry
        .resolve_runner_bin(plugin_id)
        .map_err(RunnerError::Other)?;
    let bin = bin_path.to_string_lossy().to_string();

    let plugin_cfg = registry
        .plugin_config_blob(plugin_id)
        .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string()));

    execution::run_plugin_runner_resume(
        work_dir,
        &bin,
        plugin_id,
        runner_cli,
        model,
        session_id,
        message,
        timeout,
        output_handler,
        output_stream,
        plugin_cfg,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::ExitStatus;
    use tempfile::tempdir;

    #[test]
    fn runner_output_display_redacts_output() {
        let output = RunnerOutput {
            status: ExitStatus::default(), // success usually
            stdout: "out: API_KEY=secret123".to_string(),
            stderr: "err: bearer abc123def456".to_string(),
            session_id: None,
        };
        let msg = format!("{}", output);
        assert!(msg.contains("API_KEY=[REDACTED]"));
        assert!(msg.contains("bearer [REDACTED]"));
        assert!(!msg.contains("secret123"));
        assert!(!msg.contains("abc123def456"));
    }

    #[test]
    fn output_stream_terminal_allows_terminal_output() {
        assert!(OutputStream::Terminal.streams_to_terminal());
    }

    #[test]
    fn output_stream_handler_only_suppresses_terminal_output() {
        assert!(!OutputStream::HandlerOnly.streams_to_terminal());
    }

    #[test]
    fn resume_session_missing_session_id_includes_runner_and_bin() {
        let dir = tempdir().expect("tempdir");
        let bins = RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "agent",
            kimi: "kimi",
            pi: "pi",
        };

        let err = resume_session(
            Runner::Opencode,
            dir.path(),
            bins,
            Model::Glm47,
            None,
            execution::ResolvedRunnerCliOptions::default(),
            "   ",
            "hello",
            None,
            None,
            None,
            OutputStream::HandlerOnly,
            PhaseType::Implementation,
            None,
        )
        .unwrap_err();

        let msg = format!("{err}");
        assert!(msg.contains("operation=resume_session"));
        assert!(msg.contains("runner=opencode"));
        assert!(msg.contains("bin=opencode"));
        assert!(msg.to_lowercase().contains("session_id"));
    }

    #[test]
    fn runner_requires_session_id_requires_for_all_runners() {
        // All runners including Kimi require session_id for proper resume
        assert!(runner_requires_session_id(&Runner::Kimi));
        assert!(runner_requires_session_id(&Runner::Codex));
        assert!(runner_requires_session_id(&Runner::Opencode));
        assert!(runner_requires_session_id(&Runner::Gemini));
        assert!(runner_requires_session_id(&Runner::Cursor));
        assert!(runner_requires_session_id(&Runner::Claude));
        assert!(runner_requires_session_id(&Runner::Pi));
    }

    #[test]
    fn run_prompt_invalid_model_includes_operation_and_bin() {
        let dir = tempdir().expect("tempdir");
        let bins = RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "agent",
            kimi: "kimi",
            pi: "pi",
        };

        let err = run_prompt(
            Runner::Codex,
            dir.path(),
            bins,
            Model::Glm47,
            Some(ReasoningEffort::Low),
            execution::ResolvedRunnerCliOptions::default(),
            "prompt",
            None,
            None,
            None,
            OutputStream::HandlerOnly,
            PhaseType::Implementation,
            None,
            None,
        )
        .unwrap_err();

        let msg = format!("{err}");
        assert!(msg.contains("operation=run_prompt"));
        assert!(msg.contains("runner=codex"));
        assert!(msg.contains("bin=codex"));
    }
}
