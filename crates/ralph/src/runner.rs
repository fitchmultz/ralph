//! Runner orchestration for executing tasks across supported CLIs and parsing outputs.

mod execution;

use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Model, ReasoningEffort, Runner, TaskAgent,
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

const OPENCODE_PROMPT_FILE_MESSAGE: &str = "Follow the attached prompt file verbatim.";
const DEFAULT_GEMINI_MODEL: &str = "gemini-3-flash-preview";
const DEFAULT_CLAUDE_MODEL: &str = "sonnet";
const TEMP_RETENTION: Duration = Duration::from_secs(60 * 60 * 24 * 7);

pub struct RunnerOutput {
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
pub struct AgentSettings {
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
}

pub fn resolve_agent_settings(
    runner_override: Option<Runner>,
    model_override: Option<Model>,
    effort_override: Option<ReasoningEffort>,
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

    Ok(AgentSettings {
        runner,
        model,
        reasoning_effort,
    })
}

pub fn validate_model_for_runner(runner: Runner, model: &Model) -> Result<()> {
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
}

pub fn resolve_binaries(agent: &AgentConfig) -> RunnerBinaries<'_> {
    let codex = agent.codex_bin.as_deref().unwrap_or("codex");
    let opencode = agent.opencode_bin.as_deref().unwrap_or("opencode");
    let gemini = agent.gemini_bin.as_deref().unwrap_or("gemini");
    let claude = agent.claude_bin.as_deref().unwrap_or("claude");
    RunnerBinaries {
        codex,
        opencode,
        gemini,
        claude,
    }
}

pub fn default_model_for_runner(runner: Runner) -> Model {
    match runner {
        Runner::Codex => Model::Gpt52Codex,
        Runner::Opencode => Model::Glm47,
        Runner::Gemini => Model::Custom(DEFAULT_GEMINI_MODEL.to_string()),
        Runner::Claude => Model::Custom(DEFAULT_CLAUDE_MODEL.to_string()),
    }
}

pub fn extract_final_assistant_response(stdout: &str) -> Option<String> {
    execution::extract_final_assistant_response(stdout)
}

pub fn resolve_model_for_runner(
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
pub fn run_prompt(
    runner: Runner,
    work_dir: &Path,
    bins: RunnerBinaries<'_>,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    validate_model_for_runner(runner, &model).map_err(RunnerError::Other)?;
    let output = match runner {
        Runner::Codex => execution::run_codex(
            work_dir,
            bins.codex,
            model,
            reasoning_effort,
            prompt,
            timeout,
            output_handler.clone(),
        )?,
        Runner::Opencode => execution::run_opencode(
            work_dir,
            bins.opencode,
            &model,
            prompt,
            timeout,
            output_handler.clone(),
        )?,
        Runner::Gemini => execution::run_gemini(
            work_dir,
            bins.gemini,
            model,
            prompt,
            timeout,
            output_handler.clone(),
        )?,
        Runner::Claude => execution::run_claude(
            work_dir,
            bins.claude,
            model,
            prompt,
            timeout,
            permission_mode,
            output_handler,
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
pub fn resume_session(
    runner: Runner,
    work_dir: &Path,
    bins: RunnerBinaries<'_>,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    session_id: &str,
    message: &str,
    permission_mode: Option<ClaudePermissionMode>,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    let reasoning_effort = if runner == Runner::Codex {
        reasoning_effort
    } else {
        None
    };
    validate_model_for_runner(runner, &model).map_err(RunnerError::Other)?;
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(RunnerError::Other(anyhow!("missing session_id for resume")));
    }
    let message = message.trim();
    if message.is_empty() {
        return Err(RunnerError::Other(anyhow!("missing message for resume")));
    }

    let output = match runner {
        Runner::Codex => execution::run_codex_resume(
            work_dir,
            bins.codex,
            model,
            reasoning_effort,
            session_id,
            message,
            timeout,
            output_handler,
        ),
        Runner::Opencode => execution::run_opencode_resume(
            work_dir,
            bins.opencode,
            &model,
            session_id,
            message,
            timeout,
            output_handler,
        ),
        Runner::Gemini => execution::run_gemini_resume(
            work_dir,
            bins.gemini,
            model,
            session_id,
            message,
            timeout,
            output_handler,
        ),
        Runner::Claude => execution::run_claude_resume(
            work_dir,
            bins.claude,
            model,
            session_id,
            message,
            timeout,
            permission_mode,
            output_handler,
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

pub fn parse_model(value: &str) -> Result<Model> {
    let trimmed = value.trim();
    let model = trimmed.parse::<Model>().map_err(|err| anyhow!(err))?;
    Ok(model)
}

pub fn parse_reasoning_effort(value: &str) -> Result<ReasoningEffort> {
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
}
