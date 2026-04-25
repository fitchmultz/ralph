//! Shared runner-setting resolution for task command workflows.
//!
//! Purpose:
//! - Resolve canonical runner settings for task build, update, and decomposition planning workflows.
//!
//! Responsibilities:
//! - Bridge task command options into `runner::resolve_agent_settings`.
//! - Preserve task-specific permission-mode behavior derived from resolved config.
//!
//! Not handled here:
//! - Prompt rendering.
//! - Request parsing.
//! - Queue mutation or validation.
//!
//! Usage:
//! - Called by task build, task update, and decomposition planning helpers before invoking a runner.
//!
//! Invariants/assumptions:
//! - Approval/sandbox/verbosity defaults come from resolved config unless explicitly overridden.
//! - Returned settings stay suitable for single-phase task command invocations.

use crate::commands::task::{TaskBuildOptions, TaskUpdateSettings};
use crate::contracts::{
    ClaudePermissionMode, Model, ReasoningEffort, Runner, RunnerCliOptionsPatch,
};
use crate::{config, runner};
use anyhow::Result;

#[derive(Debug, Clone)]
pub(crate) struct TaskRunnerSettings {
    pub(crate) runner: Runner,
    pub(crate) model: Model,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) runner_cli: runner::ResolvedRunnerCliOptions,
    pub(crate) permission_mode: Option<ClaudePermissionMode>,
}

pub(crate) fn resolve_task_runner_settings(
    resolved: &config::Resolved,
    runner_override: Option<Runner>,
    model_override: Option<Model>,
    reasoning_effort_override: Option<ReasoningEffort>,
    runner_cli_overrides: &RunnerCliOptionsPatch,
) -> Result<TaskRunnerSettings> {
    let settings = runner::resolve_agent_settings(
        runner_override,
        model_override,
        reasoning_effort_override,
        runner_cli_overrides,
        None,
        &resolved.config.agent,
    )?;

    Ok(TaskRunnerSettings {
        runner: settings.runner,
        model: settings.model,
        reasoning_effort: settings.reasoning_effort,
        runner_cli: settings.runner_cli,
        permission_mode: resolved.config.agent.claude_permission_mode,
    })
}

pub(crate) fn resolve_task_build_settings(
    resolved: &config::Resolved,
    opts: &TaskBuildOptions,
) -> Result<TaskRunnerSettings> {
    resolve_task_runner_settings(
        resolved,
        opts.runner_override.clone(),
        opts.model_override.clone(),
        opts.reasoning_effort_override,
        &opts.runner_cli_overrides,
    )
}

pub(crate) fn resolve_task_update_settings(
    resolved: &config::Resolved,
    settings: &TaskUpdateSettings,
) -> Result<TaskRunnerSettings> {
    resolve_task_runner_settings(
        resolved,
        settings.runner_override.clone(),
        settings.model_override.clone(),
        settings.reasoning_effort_override,
        &settings.runner_cli_overrides,
    )
}
