//! Runner orchestration for executing tasks across supported CLIs and parsing outputs.
//!
//! Responsibilities:
//! - Resolve runner binaries, validate model compatibility, and dispatch execution.
//! - Normalize runner output and surface runner-specific errors with context.
//! - Provide dual APIs: `run_prompt` (new sessions) and `resume_session` (continue existing).
//! - Centralize model resolution with cascade: override → task → config → default.
//! - Manage `AgentSettings` aggregation from multiple config sources.
//!
//! Does not handle:
//! - CLI argument parsing or queue/task selection.
//! - Persisting queue data or managing task state transitions.
//! - Actual runner CLI implementation details (delegated to `execution/` submodules).
//! - Prompt templating or composition.
//!
//! Assumptions/invariants:
//! - Runner binaries are available on PATH or configured explicitly.
//! - Model validation rules are enforced before execution starts.
//! - Session IDs are non-empty for runners that require them (all except Kimi).
//! - Messages are non-empty for resume operations.
//!
//! Size Justification (947 lines, exceeds 700 LOC heuristic):
//! This file is intentionally larger than the typical 700 LOC limit because it serves as the
//! cohesive runner orchestration module with shared concerns that would fragment if split:
//!
//! - `RunnerError` enum: 8 variants with rich error context and redaction support (42 lines).
//! - Dual APIs (`run_prompt` + `resume_session`): ~282 lines across both functions, each
//!   dispatching to 7 different runner implementations with similar but not identical signatures.
//! - 7 runner type dispatch: Codex, Opencode, Gemini, Cursor, Claude, Kimi, Pi - each needs
//!   handling in both APIs with runner-specific parameter mapping.
//! - 24 unit tests: 270 lines with comprehensive coverage of model validation, resolution,
//!   error redaction, and edge cases (tests must colocate with implementation per project rules).
//! - Model resolution logic: Complex cascade with per-runner normalization (e.g., Codex
//!   requires specific models; others use custom strings).
//!
//! Refactoring already completed: Execution logic moved to `runner/execution/` submodules.
//! What remains here: Public API surface, error types, model resolution, and tests.
//! Why not split further: Error types and model resolution are shared across all runner
//! operations; splitting would fragment cohesive logic and reduce maintainability.
//!
//! Architecture Notes:
//! - Delegation pattern: This module defines the public API and delegates to
//!   `runner/execution/` submodules for actual execution details.
//! - Runner dispatch: Large match statements in `run_prompt` and `resume_session` dispatch
//!   to runner-specific implementations in the execution submodule.
//! - Model normalization: Codex requires specific models (gpt-5.2-codex, gpt-5.2); other
//!   runners use custom model strings with defaults per runner.
//! - Test coverage: Extensive tests covering model validation, resolution, error redaction,
//!   and edge cases for all 7 supported runners.

mod execution;

pub(crate) use execution::{ctrlc_state, ResolvedRunnerCliOptions};

use crate::commands::run::PhaseType;
use crate::constants::defaults::{
    DEFAULT_CLAUDE_MODEL, DEFAULT_CURSOR_MODEL, DEFAULT_GEMINI_MODEL, OPENCODE_PROMPT_FILE_MESSAGE,
};
use crate::constants::timeouts::TEMP_RETENTION;
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

    // Warn about xhigh usage due to high consumption of usage limits
    if reasoning_effort == Some(ReasoningEffort::XHigh) {
        log::warn!(
            "Using xhigh reasoning effort. This consumes usage limits extremely fast and should only be used rarely."
        );
    }

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

/// Resolved settings for a single phase.
///
/// This struct holds the final resolved runner, model, reasoning effort, and CLI options
/// for a specific phase of execution.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedPhaseSettings {
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: execution::ResolvedRunnerCliOptions,
}

impl ResolvedPhaseSettings {
    /// Convert to AgentSettings for use with existing runner APIs.
    pub fn to_agent_settings(&self) -> AgentSettings {
        AgentSettings {
            runner: self.runner,
            model: self.model.clone(),
            reasoning_effort: self.reasoning_effort,
            runner_cli: self.runner_cli,
        }
    }
}

/// Resolved settings for all phases (1-3).
///
/// Contains per-phase resolved settings for multi-phase task execution.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PhaseSettingsMatrix {
    pub phase1: ResolvedPhaseSettings,
    pub phase2: ResolvedPhaseSettings,
    pub phase3: ResolvedPhaseSettings,
}

/// Warnings collected during phase settings resolution.
///
/// Tracks unused phase overrides and effort warnings for user feedback.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ResolutionWarnings {
    /// Phase 1 overrides were specified but will not be used
    pub unused_phase1: bool,
    /// Phase 2 overrides were specified but will not be used
    pub unused_phase2: bool,
    /// Phase 3 overrides were specified but will not be used
    pub unused_phase3: bool,
}

/// Resolve per-phase settings matrix from all configuration sources.
///
/// This function is pure (no side effects) and testable with minimal fixtures.
/// It resolves settings for all three phases following the precedence rules:
/// 1. CLI phase override (--runner-phaseN, --model-phaseN, --effort-phaseN)
/// 2. Config phase override (agent.phase_overrides.phaseN.*)
/// 3. CLI global overrides (--runner, --model, --effort)
/// 4. Task overrides (task.agent.*)
/// 5. Config defaults (agent.*)
/// 6. Code defaults
///
/// Special rules:
/// - When runner is overridden at phase level without explicit model, uses runner's default model
/// - Single-pass (--phases 1) uses Phase 2 overrides
/// - Effort is only valid for Codex runners; ignored for others
///
/// # Arguments
/// * `overrides` - CLI overrides from `AgentOverrides` (includes global and phase-specific)
/// * `config_agent` - Resolved agent configuration
/// * `task_agent` - Optional task-level agent configuration
/// * `phases` - Number of phases that will execute (1, 2, or 3)
///
/// # Returns
/// * `Ok((PhaseSettingsMatrix, ResolutionWarnings))` - Resolved settings and any warnings
/// * `Err(...)` - Validation errors (e.g., invalid model for runner in a specific phase)
pub(crate) fn resolve_phase_settings_matrix(
    overrides: &crate::agent::AgentOverrides,
    config_agent: &AgentConfig,
    task_agent: Option<&TaskAgent>,
    phases: u8,
) -> Result<(PhaseSettingsMatrix, ResolutionWarnings)> {
    let cli_phase_overrides = overrides.phase_overrides.as_ref();
    let config_phase_overrides = config_agent.phase_overrides.as_ref();

    let mut warnings = ResolutionWarnings::default();

    // Collect warnings for unused phase overrides
    // Phase usage by execution mode:
    // - phases=1 (single-pass): uses Phase 2 settings
    // - phases=2: uses Phase 1 (plan) and Phase 2 (implement)
    // - phases=3: uses all phases
    if phases < 3 {
        // Phase 3 overrides are unused when running less than 3 phases
        if let Some(cli) = cli_phase_overrides {
            if cli.phase3.is_some() {
                warnings.unused_phase3 = true;
            }
        }
        if let Some(config) = config_phase_overrides {
            if config.phase3.is_some() {
                warnings.unused_phase3 = true;
            }
        }
    }
    if phases < 2 {
        // Phase 1 overrides are unused in single-pass (uses Phase 2 instead)
        if let Some(cli) = cli_phase_overrides {
            if cli.phase1.is_some() {
                warnings.unused_phase1 = true;
            }
        }
        if let Some(config) = config_phase_overrides {
            if config.phase1.is_some() {
                warnings.unused_phase1 = true;
            }
        }
    }
    // Note: Phase 2 is always used (single-pass uses Phase 2, 2-phase uses Phase 2, 3-phase uses Phase 2)

    // Resolve each phase
    let phase1 = resolve_single_phase(
        1,
        cli_phase_overrides.and_then(|p| p.phase1.as_ref()),
        config_phase_overrides.and_then(|p| p.phase1.as_ref()),
        overrides,
        task_agent,
        config_agent,
    )
    .map_err(|e| anyhow!("Phase 1: {}", e))?;

    let phase2 = resolve_single_phase(
        2,
        cli_phase_overrides.and_then(|p| p.phase2.as_ref()),
        config_phase_overrides.and_then(|p| p.phase2.as_ref()),
        overrides,
        task_agent,
        config_agent,
    )
    .map_err(|e| anyhow!("Phase 2: {}", e))?;

    let phase3 = resolve_single_phase(
        3,
        cli_phase_overrides.and_then(|p| p.phase3.as_ref()),
        config_phase_overrides.and_then(|p| p.phase3.as_ref()),
        overrides,
        task_agent,
        config_agent,
    )
    .map_err(|e| anyhow!("Phase 3: {}", e))?;

    Ok((
        PhaseSettingsMatrix {
            phase1,
            phase2,
            phase3,
        },
        warnings,
    ))
}

/// Resolve settings for a single phase following precedence rules.
///
/// Precedence (highest to lowest):
/// 1. CLI phase override
/// 2. Config phase override
/// 3. CLI global override
/// 4. Task override
/// 5. Config default
/// 6. Code default
fn resolve_single_phase(
    phase: u8,
    cli_phase_override: Option<&crate::contracts::PhaseOverrideConfig>,
    config_phase_override: Option<&crate::contracts::PhaseOverrideConfig>,
    global_overrides: &crate::agent::AgentOverrides,
    task_agent: Option<&TaskAgent>,
    config_agent: &AgentConfig,
) -> Result<ResolvedPhaseSettings> {
    // Determine if runner was overridden at phase level
    let runner_overridden_at_phase = cli_phase_override.and_then(|p| p.runner).is_some()
        || config_phase_override.and_then(|p| p.runner).is_some();

    // Resolve runner with precedence: CLI phase > config phase > CLI global > task > config > default
    let runner = cli_phase_override
        .and_then(|p| p.runner)
        .or(config_phase_override.and_then(|p| p.runner))
        .or(global_overrides.runner)
        .or(task_agent.and_then(|a| a.runner))
        .or(config_agent.runner)
        .unwrap_or_default();

    // Resolve model with precedence: CLI phase > config phase > CLI global > task > config (with defaulting)
    let model = resolve_model_for_phase(
        runner,
        cli_phase_override.and_then(|p| p.model.clone()),
        config_phase_override.and_then(|p| p.model.clone()),
        global_overrides.model.clone(),
        task_agent.and_then(|a| a.model.clone()),
        config_agent.model.clone(),
        runner_overridden_at_phase || global_overrides.runner.is_some(),
    );

    // Validate model for runner
    validate_model_for_runner(runner, &model).map_err(|e| {
        anyhow!(
            "invalid model {} for {} runner: {}",
            model.as_str(),
            runner_label(runner),
            e
        )
    })?;

    // Resolve reasoning effort (only for Codex)
    let reasoning_effort = resolve_phase_reasoning_effort(
        runner,
        cli_phase_override.and_then(|p| p.reasoning_effort),
        config_phase_override.and_then(|p| p.reasoning_effort),
        global_overrides.reasoning_effort,
        task_agent,
        config_agent.reasoning_effort,
    );

    // Warn about xhigh usage
    if reasoning_effort == Some(ReasoningEffort::XHigh) {
        log::warn!(
            "Phase {}: Using xhigh reasoning effort. This consumes usage limits extremely fast and should only be used rarely.",
            phase
        );
    }

    // Resolve runner CLI options (use global overrides for now; phase-specific CLI overrides not yet implemented)
    let runner_cli = execution::resolve_runner_cli_options(
        runner,
        &global_overrides.runner_cli,
        task_agent.and_then(|a| a.runner_cli.as_ref()),
        config_agent,
    )?;

    Ok(ResolvedPhaseSettings {
        runner,
        model,
        reasoning_effort,
        runner_cli,
    })
}

/// Resolve model for a phase with proper precedence and defaulting.
///
/// When runner is overridden at phase level or globally, and no model is explicitly
/// set at an equal-or-higher precedence, the model becomes the runner's default.
fn resolve_model_for_phase(
    runner: Runner,
    cli_phase_model: Option<Model>,
    config_phase_model: Option<Model>,
    cli_global_model: Option<Model>,
    task_model: Option<Model>,
    config_model: Option<Model>,
    runner_was_overridden: bool,
) -> Model {
    // Check for explicit model at each precedence level
    if let Some(model) = cli_phase_model {
        return model;
    }
    if let Some(model) = config_phase_model {
        return normalize_model_for_runner(runner, model);
    }
    if let Some(model) = cli_global_model {
        return model;
    }
    if let Some(model) = task_model {
        return normalize_model_for_runner(runner, model);
    }

    // If runner was overridden but no explicit model, use runner's default
    if runner_was_overridden {
        return default_model_for_runner(runner);
    }

    // Fall back to config model with normalization
    match config_model {
        None => default_model_for_runner(runner),
        Some(model) => normalize_model_for_runner(runner, model),
    }
}

/// Normalize a model for a specific runner.
fn normalize_model_for_runner(runner: Runner, model: Model) -> Model {
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
}

/// Resolve reasoning effort for a phase.
///
/// Returns Some(effort) for Codex runners, None for others.
fn resolve_phase_reasoning_effort(
    runner: Runner,
    cli_phase_effort: Option<ReasoningEffort>,
    config_phase_effort: Option<ReasoningEffort>,
    cli_global_effort: Option<ReasoningEffort>,
    task_agent: Option<&TaskAgent>,
    config_effort: Option<ReasoningEffort>,
) -> Option<ReasoningEffort> {
    // If not Codex, effort is always None.
    if runner != Runner::Codex {
        return None;
    }

    // For Codex, resolve with precedence
    let effort = cli_phase_effort
        .or(config_phase_effort)
        .or(cli_global_effort)
        .or(task_agent.and_then(|a| a.model_effort.as_reasoning_effort()))
        .or(config_effort)
        .unwrap_or_default();

    Some(effort)
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
        Runner::Kimi => Model::Custom("kimi-for-coding".to_string()),
        Runner::Pi => Model::Custom("gpt-5.2".to_string()),
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
    if runner_requires_session_id(runner) && session_id.is_empty() {
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

fn runner_requires_session_id(runner: Runner) -> bool {
    runner != Runner::Kimi
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
    fn resolve_model_for_runner_defaults_for_kimi() {
        let model = resolve_model_for_runner(Runner::Kimi, None, None, None, false);
        assert_eq!(model.as_str(), "kimi-for-coding");
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_pi() {
        let model = resolve_model_for_runner(Runner::Pi, None, None, None, false);
        assert_eq!(model.as_str(), "gpt-5.2");
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
    fn runner_requires_session_id_allows_kimi_continue() {
        assert!(!runner_requires_session_id(Runner::Kimi));
        assert!(runner_requires_session_id(Runner::Codex));
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
