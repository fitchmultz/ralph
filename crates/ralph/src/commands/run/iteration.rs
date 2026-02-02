//! Iteration handling for multi-iteration task execution.
//!
//! Responsibilities:
//! - Resolve iteration count from task config (task > config > default).
//! - Apply follow-up reasoning effort overrides for subsequent iterations.
//!
//! Not handled here:
//! - Actual iteration loop execution (handled in `run_one_impl`).
//! - Runner invocation (handled by `phases` module).
//!
//! Invariants/assumptions:
//! - Iteration count must be >= 1 (validated).
//! - Follow-up reasoning effort only applies to Codex runner.

use crate::contracts::{AgentConfig, ReasoningEffort, Task};
use crate::runner;
use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IterationSettings {
    pub count: u8,
    pub followup_reasoning_effort: Option<ReasoningEffort>,
}

/// Resolve iteration settings from task config hierarchy (task > config > default).
pub(crate) fn resolve_iteration_settings(
    task: &Task,
    config_agent: &AgentConfig,
) -> Result<IterationSettings> {
    let count = task
        .agent
        .as_ref()
        .and_then(|agent| agent.iterations)
        .or(config_agent.iterations)
        .unwrap_or(1);

    if count == 0 {
        bail!(
            "Invalid iterations for task {}: iterations must be >= 1.",
            task.id.trim()
        );
    }

    let followup_reasoning_effort = task
        .agent
        .as_ref()
        .and_then(|agent| agent.followup_reasoning_effort)
        .or(config_agent.followup_reasoning_effort);

    Ok(IterationSettings {
        count,
        followup_reasoning_effort,
    })
}

/// Apply follow-up reasoning effort to settings for subsequent iterations.
/// Only affects Codex runner; other runners log a warning if configured.
pub(crate) fn apply_followup_reasoning_effort(
    base_settings: &runner::AgentSettings,
    followup_reasoning_effort: Option<ReasoningEffort>,
    is_followup: bool,
) -> runner::AgentSettings {
    if !is_followup {
        return base_settings.clone();
    }

    let mut settings = base_settings.clone();
    if let Some(effort) = followup_reasoning_effort {
        if settings.runner == crate::contracts::Runner::Codex {
            settings.reasoning_effort = Some(effort);
        } else {
            log::warn!(
                "Follow-up reasoning_effort configured, but runner {:?} does not support it; ignoring override.",
                settings.runner
            );
        }
    }
    settings
}
