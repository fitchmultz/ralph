//! Agent settings aggregation and phase-aware resolution.
//!
//! Responsibilities:
//! - Resolve `AgentSettings` from override/task/config sources.
//! - Resolve per-phase settings (`PhaseSettingsMatrix`) with precedence rules.
//! - Produce `ResolutionWarnings` for unused overrides.
//!
//! Does not handle:
//! - Runner execution dispatch (see `runner.rs`).
//! - Model validation rules implementation (delegated to `runner/model.rs`).
//! - Runner CLI command assembly/execution (see `runner/execution/*`).
//!
//! Assumptions/invariants:
//! - Model validation is enforced before execution.
//! - Reasoning effort only applies to Codex and is ignored otherwise.

use anyhow::{Result, anyhow};

use crate::contracts::{
    AgentConfig, Model, ReasoningEffort, Runner, RunnerCliOptionsPatch, TaskAgent,
};

use super::execution;
use super::model;

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
    let runner_was_overridden = runner_override.is_some();

    let runner = runner_override
        .or(task_agent.and_then(|a| a.runner.clone()))
        .or(config_agent.runner.clone())
        .unwrap_or_default();

    let model = model::resolve_model_for_runner(
        &runner,
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

    if reasoning_effort == Some(ReasoningEffort::XHigh) {
        log::warn!(
            "Using xhigh reasoning effort. This consumes usage limits extremely fast and should only be used rarely."
        );
    }

    model::validate_model_for_runner(&runner, &model)?;

    let runner_cli = execution::resolve_runner_cli_options(
        &runner,
        runner_cli_override,
        task_agent.and_then(|a| a.runner_cli.as_ref()),
        config_agent,
    )?;

    Ok(AgentSettings {
        runner: runner.clone(),
        model,
        reasoning_effort,
        runner_cli,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedPhaseSettings {
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: execution::ResolvedRunnerCliOptions,
}

impl ResolvedPhaseSettings {
    pub fn to_agent_settings(&self) -> AgentSettings {
        AgentSettings {
            runner: self.runner.clone(),
            model: self.model.clone(),
            reasoning_effort: self.reasoning_effort,
            runner_cli: self.runner_cli,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PhaseSettingsMatrix {
    pub phase1: ResolvedPhaseSettings,
    pub phase2: ResolvedPhaseSettings,
    pub phase3: ResolvedPhaseSettings,
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct ResolutionWarnings {
    pub unused_phase1: bool,
    pub unused_phase2: bool,
    pub unused_phase3: bool,
}

pub(crate) fn resolve_phase_settings_matrix(
    overrides: &crate::agent::AgentOverrides,
    config_agent: &AgentConfig,
    task_agent: Option<&TaskAgent>,
    phases: u8,
) -> Result<(PhaseSettingsMatrix, ResolutionWarnings)> {
    let cli_phase_overrides = overrides.phase_overrides.as_ref();
    let config_phase_overrides = config_agent.phase_overrides.as_ref();

    let mut warnings = ResolutionWarnings::default();

    if phases < 3 {
        if let Some(cli) = cli_phase_overrides
            && cli.phase3.is_some()
        {
            warnings.unused_phase3 = true;
        }
        if let Some(config) = config_phase_overrides
            && config.phase3.is_some()
        {
            warnings.unused_phase3 = true;
        }
    }
    if phases < 2 {
        if let Some(cli) = cli_phase_overrides
            && cli.phase1.is_some()
        {
            warnings.unused_phase1 = true;
        }
        if let Some(config) = config_phase_overrides
            && config.phase1.is_some()
        {
            warnings.unused_phase1 = true;
        }
    }

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

fn resolve_single_phase(
    phase: u8,
    cli_phase_override: Option<&crate::contracts::PhaseOverrideConfig>,
    config_phase_override: Option<&crate::contracts::PhaseOverrideConfig>,
    global_overrides: &crate::agent::AgentOverrides,
    task_agent: Option<&TaskAgent>,
    config_agent: &AgentConfig,
) -> Result<ResolvedPhaseSettings> {
    let runner_overridden_at_phase = cli_phase_override.and_then(|p| p.runner.clone()).is_some()
        || config_phase_override
            .and_then(|p| p.runner.clone())
            .is_some();

    let runner = cli_phase_override
        .and_then(|p| p.runner.clone())
        .or(config_phase_override.and_then(|p| p.runner.clone()))
        .or(global_overrides.runner.clone())
        .or(task_agent.and_then(|a| a.runner.clone()))
        .or(config_agent.runner.clone())
        .unwrap_or_default();

    let model_value = model::resolve_model_for_phase(
        &runner,
        cli_phase_override.and_then(|p| p.model.clone()),
        config_phase_override.and_then(|p| p.model.clone()),
        global_overrides.model.clone(),
        task_agent.and_then(|a| a.model.clone()),
        config_agent.model.clone(),
        runner_overridden_at_phase || global_overrides.runner.is_some(),
    );

    model::validate_model_for_runner(&runner, &model_value).map_err(|e| {
        anyhow!(
            "invalid model {} for {} runner: {}",
            model_value.as_str(),
            runner.as_str(),
            e
        )
    })?;

    let reasoning_effort = resolve_phase_reasoning_effort(
        &runner,
        cli_phase_override.and_then(|p| p.reasoning_effort),
        config_phase_override.and_then(|p| p.reasoning_effort),
        global_overrides.reasoning_effort,
        task_agent,
        config_agent.reasoning_effort,
    );

    if reasoning_effort == Some(ReasoningEffort::XHigh) {
        log::warn!(
            "Phase {}: Using xhigh reasoning effort. This consumes usage limits extremely fast and should be used rarely.",
            phase
        );
    }

    let runner_cli = execution::resolve_runner_cli_options(
        &runner,
        &global_overrides.runner_cli,
        task_agent.and_then(|a| a.runner_cli.as_ref()),
        config_agent,
    )?;

    Ok(ResolvedPhaseSettings {
        runner: runner.clone(),
        model: model_value,
        reasoning_effort,
        runner_cli,
    })
}

fn resolve_phase_reasoning_effort(
    runner: &Runner,
    cli_phase_effort: Option<ReasoningEffort>,
    config_phase_effort: Option<ReasoningEffort>,
    cli_global_effort: Option<ReasoningEffort>,
    task_agent: Option<&TaskAgent>,
    config_effort: Option<ReasoningEffort>,
) -> Option<ReasoningEffort> {
    if runner != &Runner::Codex {
        return None;
    }

    let effort = cli_phase_effort
        .or(config_phase_effort)
        .or(cli_global_effort)
        .or(task_agent.and_then(|a| a.model_effort.as_reasoning_effort()))
        .or(config_effort)
        .unwrap_or_default();

    Some(effort)
}
