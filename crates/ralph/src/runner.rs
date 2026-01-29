//! Runner orchestration for executing tasks across supported CLIs and parsing outputs.
//!
//! Responsibilities:
//! - Resolve runner binaries, validate model compatibility, and dispatch execution.
//! - Normalize runner output and surface runner-specific errors with context.
//!
//! Does not handle:
//! - CLI argument parsing or queue/task selection.
//! - Persisting queue data or managing task state transitions.
//!
//! Assumptions/invariants:
//! - Runner binaries are available on PATH or configured explicitly.
//! - Model validation rules are enforced before execution starts.

mod execution;

pub(crate) use execution::ResolvedRunnerCliOptions;

use crate::commands::run::PhaseType;
use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Model, ReasoningEffort, Runner, RunnerCliOptionsPatch,
    TaskAgent,
};
use crate::redaction::{redact_text, RedactedString};
use anyhow::{anyhow, bail, Result};
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

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("runner binary not found: {bin}")]
    BinaryMissing {
        bin: String,
        #[source]
        source: std::io::Error,
    },

    #[error("runner failed to spawn: {bin}")]
    SpawnFailed {
        bin: String,
        #[source]
        source: std::io::Error,
    },

    #[error("runner exited non-zero (code={code})\nstdout: {stdout}\nstderr: {stderr}")]
    NonZeroExit {
        code: i32,
        stdout: RedactedString,
        stderr: RedactedString,
        session_id: Option<String>,
    },

    #[error("runner terminated by signal\nstdout: {stdout}\nstderr: {stderr}")]
    TerminatedBySignal {
        stdout: RedactedString,
        stderr: RedactedString,
        session_id: Option<String>,
    },

    #[error("runner interrupted")]
    Interrupted,

    #[error("runner timed out")]
    Timeout,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("other error: {0}")]
    Other(#[from] anyhow::Error),
}

fn runner_label(runner: Runner) -> &'static str {
    match runner {
        Runner::Codex => "codex",
        Runner::Opencode => "opencode",
        Runner::Gemini => "gemini",
        Runner::Cursor => "cursor",
        Runner::Claude => "claude",
        Runner::Kimi => "kimi",
        Runner::Pi => "pi",
    }
}

pub(crate) fn runner_execution_error(runner: Runner, bin: &str, step: &str) -> RunnerError {
    RunnerError::Other(anyhow!(
        "Runner execution failed (runner={}, bin={}): {}.",
        runner_label(runner),
        bin,
        step
    ))
}

pub(crate) fn runner_execution_error_with_source(
    runner: Runner,
    bin: &str,
    step: &str,
    source: impl fmt::Display,
) -> RunnerError {
    RunnerError::Other(anyhow!(
        "Runner execution failed (runner={}, bin={}): {}: {}.",
        runner_label(runner),
        bin,
        step,
        source
    ))
}

const OPENCODE_PROMPT_FILE_MESSAGE: &str = "Follow the attached prompt file verbatim.";
const DEFAULT_GEMINI_MODEL: &str = "gemini-3-flash-preview";
const DEFAULT_CLAUDE_MODEL: &str = "sonnet";
const DEFAULT_CURSOR_MODEL: &str = "auto";
const TEMP_RETENTION: Duration = Duration::from_secs(60 * 60 * 24 * 7);

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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AgentSettings {
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: execution::ResolvedRunnerCliOptions,
}

pub(crate) fn resolve_agent_settings(
    runner_override: Option<Runner>,
    model_override: Option<Model>,
    effort_override: Option<ReasoningEffort>,
    runner_cli_override: &RunnerCliOptionsPatch,
    task_agent: Option<&TaskAgent>,
    config_agent: &AgentConfig,
) -> Result<AgentSettings> {
    let runner = runner_override
        .or(task_agent.and_then(|a| a.runner))
        .or(config_agent.runner)
        .unwrap_or_default();

    let runner_was_overridden = runner_override.is_some();

    let model = resolve_model_for_runner(
        runner,
        model_override,
        task_agent.and_then(|a| a.model.clone()),
        config_agent.model.clone(),
        runner_was_overridden,
    );

    let effort_candidate = effort_override
        .or(task_agent.and_then(|a| a.model_effort.as_reasoning_effort()))
        .or(config_agent.reasoning_effort);

    let reasoning_effort = if runner == Runner::Codex {
        Some(effort_candidate.unwrap_or_default())
    } else {
        None
    };

    validate_model_for_runner(runner, &model)?;

    let runner_cli = execution::resolve_runner_cli_options(
        runner,
        runner_cli_override,
        task_agent.and_then(|a| a.runner_cli.as_ref()),
        config_agent,
    )?;

    Ok(AgentSettings {
        runner,
        model,
        reasoning_effort,
        runner_cli,
    })
}

pub(crate) fn validate_model_for_runner(runner: Runner, model: &Model) -> Result<()> {
    if runner == Runner::Codex {
        match model {
            Model::Gpt52Codex | Model::Gpt52 => {}
            Model::Glm47 => {
                bail!("model zai-coding-plan/glm-4.7 is not supported for codex runner")
            }
            Model::Custom(name) => bail!(
                "model {} is not supported for codex runner (allowed: gpt-5.2-codex, gpt-5.2)",
                name
            ),
        }
    }
    Ok(())
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

pub(crate) fn resolve_binaries(agent: &AgentConfig) -> RunnerBinaries<'_> {
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

pub(crate) fn default_model_for_runner(runner: Runner) -> Model {
    match runner {
        Runner::Codex => Model::Gpt52Codex,
        Runner::Opencode => Model::Glm47,
        Runner::Gemini => Model::Custom(DEFAULT_GEMINI_MODEL.to_string()),
        Runner::Cursor => Model::Custom(DEFAULT_CURSOR_MODEL.to_string()),
        Runner::Claude => Model::Custom(DEFAULT_CLAUDE_MODEL.to_string()),
        Runner::Kimi => Model::Custom("kimi-k2".to_string()),
        Runner::Pi => Model::Custom("pi-default".to_string()),
    }
}

pub(crate) fn extract_final_assistant_response(stdout: &str) -> Option<String> {
    execution::extract_final_assistant_response(stdout)
}

pub(crate) fn resolve_model_for_runner(
    runner: Runner,
    override_model: Option<Model>,
    task_model: Option<Model>,
    config_model: Option<Model>,
    runner_was_overridden: bool,
) -> Model {
    let normalize_model = |model: Model| {
        if runner == Runner::Codex {
            match model {
                Model::Gpt52Codex | Model::Gpt52 => model,
                _ => default_model_for_runner(runner),
            }
        } else if model == Model::Gpt52Codex {
            default_model_for_runner(runner)
        } else {
            model
        }
    };

    if let Some(model) = override_model {
        return model;
    }
    if let Some(model) = task_model {
        return normalize_model(model);
    }

    if runner_was_overridden {
        return default_model_for_runner(runner);
    }

    match config_model {
        None => default_model_for_runner(runner),
        Some(model) => normalize_model(model),
    }
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
) -> Result<RunnerOutput, RunnerError> {
    let bin = match runner {
        Runner::Codex => bins.codex,
        Runner::Opencode => bins.opencode,
        Runner::Gemini => bins.gemini,
        Runner::Cursor => bins.cursor,
        Runner::Claude => bins.claude,
        Runner::Kimi => bins.kimi,
        Runner::Pi => bins.pi,
    };
    validate_model_for_runner(runner, &model).map_err(|err| {
        RunnerError::Other(anyhow!(
            "Runner configuration error (operation=run_prompt, runner={}, bin={}): {}",
            runner_label(runner),
            bin,
            err
        ))
    })?;
    let output = match runner {
        Runner::Codex => execution::run_codex(
            work_dir,
            bins.codex,
            runner_cli,
            model,
            reasoning_effort,
            prompt,
            timeout,
            output_handler.clone(),
            output_stream,
        )?,
        Runner::Opencode => execution::run_opencode(
            work_dir,
            bins.opencode,
            runner_cli,
            &model,
            prompt,
            timeout,
            output_handler.clone(),
            output_stream,
        )?,
        Runner::Gemini => execution::run_gemini(
            work_dir,
            bins.gemini,
            runner_cli,
            model,
            prompt,
            timeout,
            output_handler.clone(),
            output_stream,
        )?,
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
        Runner::Claude => execution::run_claude(
            work_dir,
            bins.claude,
            runner_cli,
            model,
            prompt,
            timeout,
            runner_cli.effective_claude_permission_mode(permission_mode),
            output_handler,
            output_stream,
        )?,
        Runner::Kimi => execution::run_kimi(
            work_dir,
            bins.kimi,
            runner_cli,
            model,
            prompt,
            timeout,
            output_handler.clone(),
            output_stream,
        )?,
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
) -> Result<RunnerOutput, RunnerError> {
    let reasoning_effort = if runner == Runner::Codex {
        reasoning_effort
    } else {
        None
    };
    let bin = match runner {
        Runner::Codex => bins.codex,
        Runner::Opencode => bins.opencode,
        Runner::Gemini => bins.gemini,
        Runner::Cursor => bins.cursor,
        Runner::Claude => bins.claude,
        Runner::Kimi => bins.kimi,
        Runner::Pi => bins.pi,
    };
    validate_model_for_runner(runner, &model).map_err(|err| {
        RunnerError::Other(anyhow!(
            "Runner configuration error (operation=resume_session, runner={}, bin={}): {}",
            runner_label(runner),
            bin,
            err
        ))
    })?;
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(RunnerError::Other(anyhow!(
            "Runner input error (operation=resume_session, runner={}, bin={}): session_id is required (non-empty). Example: --resume <SESSION_ID>.",
            runner_label(runner),
            bin
        )));
    }
    let message = message.trim();
    if message.is_empty() {
        return Err(RunnerError::Other(anyhow!(
            "Runner input error (operation=resume_session, runner={}, bin={}): message is required (non-empty).",
            runner_label(runner),
            bin
        )));
    }

    let output = match runner {
        Runner::Codex => execution::run_codex_resume(
            work_dir,
            bins.codex,
            runner_cli,
            model,
            reasoning_effort,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
        ),
        Runner::Opencode => execution::run_opencode_resume(
            work_dir,
            bins.opencode,
            runner_cli,
            &model,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
        ),
        Runner::Gemini => execution::run_gemini_resume(
            work_dir,
            bins.gemini,
            runner_cli,
            model,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
        ),
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
        Runner::Claude => execution::run_claude_resume(
            work_dir,
            bins.claude,
            runner_cli,
            model,
            session_id,
            message,
            timeout,
            runner_cli.effective_claude_permission_mode(permission_mode),
            output_handler,
            output_stream,
        ),
        Runner::Kimi => execution::run_kimi_resume(
            work_dir,
            bins.kimi,
            runner_cli,
            model,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
        ),
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

pub(crate) fn parse_model(value: &str) -> Result<Model> {
    let trimmed = value.trim();
    let model = trimmed.parse::<Model>().map_err(|err| anyhow!(err))?;
    Ok(model)
}

pub(crate) fn parse_reasoning_effort(value: &str) -> Result<ReasoningEffort> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "low" => Ok(ReasoningEffort::Low),
        "medium" => Ok(ReasoningEffort::Medium),
        "high" => Ok(ReasoningEffort::High),
        "xhigh" => Ok(ReasoningEffort::XHigh),
        _ => bail!(
            "unsupported reasoning effort: {} (allowed: low, medium, high, xhigh)",
            value.trim()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::ExitStatus;
    use tempfile::tempdir;

    #[test]
    fn validate_model_for_runner_rejects_glm47_on_codex() {
        let err = validate_model_for_runner(Runner::Codex, &Model::Glm47).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("zai-coding-plan/glm-4.7"));
    }

    #[test]
    fn validate_model_for_runner_rejects_custom_on_codex() {
        let model = Model::Custom("gemini-3-pro-preview".to_string());
        let err = validate_model_for_runner(Runner::Codex, &model).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("gemini-3-pro-preview"));
        assert!(msg.contains("gpt-5.2-codex"));
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_gemini() {
        let model = resolve_model_for_runner(Runner::Gemini, None, None, None, false);
        assert_eq!(model.as_str(), DEFAULT_GEMINI_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_replaces_codex_default_for_gemini() {
        let model =
            resolve_model_for_runner(Runner::Gemini, None, None, Some(Model::Gpt52Codex), false);
        assert_eq!(model.as_str(), DEFAULT_GEMINI_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_codex_when_config_incompatible() {
        let model = resolve_model_for_runner(
            Runner::Codex,
            None,
            None,
            Some(Model::Custom("sonnet".to_string())),
            false,
        );
        assert_eq!(model, Model::Gpt52Codex);
    }

    #[test]
    fn resolve_model_for_runner_normalizes_task_model_for_codex() {
        let model = resolve_model_for_runner(
            Runner::Codex,
            None,
            Some(Model::Custom("sonnet".to_string())),
            None,
            false,
        );
        assert_eq!(model, Model::Gpt52Codex);
    }

    #[test]
    fn resolve_model_for_runner_normalizes_task_model_for_opencode() {
        let model =
            resolve_model_for_runner(Runner::Opencode, None, Some(Model::Gpt52Codex), None, false);
        assert_eq!(model, Model::Glm47);
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_claude() {
        let model = resolve_model_for_runner(Runner::Claude, None, None, None, false);
        assert_eq!(model.as_str(), DEFAULT_CLAUDE_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_cursor() {
        let model = resolve_model_for_runner(Runner::Cursor, None, None, None, false);
        assert_eq!(model.as_str(), DEFAULT_CURSOR_MODEL);
    }

    #[test]
    fn parse_reasoning_effort_accepts_xhigh() {
        let effort = parse_reasoning_effort(" xhigh ").expect("xhigh effort");
        assert_eq!(effort, ReasoningEffort::XHigh);
    }

    #[test]
    fn parse_reasoning_effort_rejects_minimal() {
        let err = parse_reasoning_effort("minimal").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("allowed: low, medium, high, xhigh"));
    }

    #[test]
    fn runner_error_nonzero_exit_redacts_output() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "out: API_KEY=secret123".into(),
            stderr: "err: bearer abc123def456".into(),
            session_id: None,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("API_KEY=[REDACTED]"));
        assert!(msg.contains("bearer [REDACTED]"));
        assert!(!msg.contains("secret123"));
        assert!(!msg.contains("abc123def456"));
    }

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
    fn resolve_model_for_runner_override_uses_runner_default_when_no_model() {
        // When runner is overridden but model is not, use runner's default
        // instead of falling back to config's model
        let model = resolve_model_for_runner(
            Runner::Opencode,
            None,
            None,
            Some(Model::Custom("sonnet".to_string())),
            true, // runner was overridden
        );
        assert_eq!(model, Model::Glm47);
    }

    #[test]
    fn resolve_model_for_runner_override_with_explicit_model() {
        // When both runner and model are overridden, use the explicit model
        let model = resolve_model_for_runner(
            Runner::Opencode,
            Some(Model::Gpt52),
            None,
            Some(Model::Custom("sonnet".to_string())),
            true,
        );
        assert_eq!(model, Model::Gpt52);
    }

    #[test]
    fn resolve_model_for_runner_no_override_uses_config_model() {
        // When runner is not overridden, use config model (with normalization)
        let model = resolve_model_for_runner(
            Runner::Codex,
            None,
            None,
            Some(Model::Gpt52Codex),
            false, // runner was not overridden
        );
        assert_eq!(model, Model::Gpt52Codex);
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
    fn runner_execution_error_includes_context() {
        let err = runner_execution_error(Runner::Gemini, "gemini", "capture child stdout");
        let msg = format!("{err}");
        assert!(msg.contains("runner=gemini"));
        assert!(msg.contains("bin=gemini"));
        assert!(msg.contains("capture child stdout"));
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
            ResolvedRunnerCliOptions::default(),
            "   ",
            "hello",
            None,
            None,
            None,
            OutputStream::HandlerOnly,
            PhaseType::Implementation,
        )
        .unwrap_err();

        let msg = format!("{err}");
        assert!(msg.contains("operation=resume_session"));
        assert!(msg.contains("runner=opencode"));
        assert!(msg.contains("bin=opencode"));
        assert!(msg.to_lowercase().contains("session_id"));
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
            ResolvedRunnerCliOptions::default(),
            "prompt",
            None,
            None,
            None,
            OutputStream::HandlerOnly,
            PhaseType::Implementation,
        )
        .unwrap_err();

        let msg = format!("{err}");
        assert!(msg.contains("operation=run_prompt"));
        assert!(msg.contains("runner=codex"));
        assert!(msg.contains("bin=codex"));
    }
}
