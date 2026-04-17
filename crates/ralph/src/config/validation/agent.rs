//! Agent validation rules.
//!
//! Responsibilities:
//! - Validate agent-specific numeric limits and binary path overrides.
//! - Expose helpers used by trust validation to identify execution-sensitive settings.
//!
//! Not handled here:
//! - Queue thresholds or git ref validation.
//! - Full config version or parallel workspace rules.
//!
//! Invariants/assumptions:
//! - Empty binary-path strings are invalid when provided.
//! - Agent phases stay within the configured global limits.

use super::ci_gate::validate_ci_gate_config;
use crate::constants::runner::{MAX_PHASES, MIN_ITERATIONS, MIN_PHASES};
use crate::contracts::{AgentConfig, PhaseOverrides, Runner, validate_webhook_settings};
use anyhow::{Result, bail};

pub fn validate_agent_binary_paths(agent: &AgentConfig, label: &str) -> Result<()> {
    macro_rules! check_bin {
        ($field:ident) => {
            if let Some(bin) = &agent.$field
                && bin.trim().is_empty()
            {
                bail!(
                    "Empty {label}.{}: binary path is required if specified.",
                    stringify!($field)
                );
            }
        };
    }

    check_bin!(codex_bin);
    check_bin!(opencode_bin);
    check_bin!(gemini_bin);
    check_bin!(claude_bin);
    check_bin!(cursor_bin);
    check_bin!(kimi_bin);
    check_bin!(pi_bin);

    Ok(())
}

pub fn validate_agent_patch(agent: &AgentConfig, label: &str) -> Result<()> {
    if let Some(phases) = agent.phases
        && !(MIN_PHASES..=MAX_PHASES).contains(&phases)
    {
        bail!(
            "Invalid {label}.phases: {phases}. Supported values are {MIN_PHASES}, {}, or {MAX_PHASES}.",
            MIN_PHASES + 1
        );
    }

    if let Some(iterations) = agent.iterations
        && iterations < MIN_ITERATIONS
    {
        bail!(
            "Invalid {label}.iterations: {iterations}. Iterations must be at least {MIN_ITERATIONS}."
        );
    }

    if let Some(timeout) = agent.session_timeout_hours
        && timeout == 0
    {
        bail!(
            "Invalid {label}.session_timeout_hours: {timeout}. Session timeout must be greater than 0."
        );
    }

    validate_agent_binary_paths(agent, label)?;
    validate_ci_gate_config(agent.ci_gate.as_ref(), label)?;
    if let Err(err) = validate_webhook_settings(&agent.webhook) {
        bail!("Invalid {label}.webhook: {err:#}");
    }
    Ok(())
}

pub(crate) fn agent_has_execution_settings(agent: &AgentConfig) -> bool {
    agent.ci_gate.is_some()
        || agent.codex_bin.is_some()
        || agent.opencode_bin.is_some()
        || agent.gemini_bin.is_some()
        || agent.claude_bin.is_some()
        || agent.cursor_bin.is_some()
        || agent.kimi_bin.is_some()
        || agent.pi_bin.is_some()
        || agent.runner.as_ref().is_some_and(Runner::is_plugin)
        || agent
            .phase_overrides
            .as_ref()
            .is_some_and(phase_overrides_have_plugin_runner)
}

fn phase_overrides_have_plugin_runner(overrides: &PhaseOverrides) -> bool {
    [&overrides.phase1, &overrides.phase2, &overrides.phase3]
        .into_iter()
        .flatten()
        .filter_map(|phase| phase.runner.as_ref())
        .any(Runner::is_plugin)
}
