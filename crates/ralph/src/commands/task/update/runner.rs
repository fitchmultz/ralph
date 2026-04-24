//! Prompt rendering and runner execution for task updates.
//!
//! Purpose:
//! - Prompt rendering and runner execution for task updates.
//!
//! Responsibilities:
//! - Build the task-updater prompt with repo-prompt and instruction-file wrappers.
//! - Resolve runner settings and execute the configured agent for task updates.
//! - Keep task-update-specific runner error messaging centralized.
//!
//! Not handled here:
//! - Queue locking, backup management, or validation.
//! - Dry-run preview printing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Prompt rendering must match the real task-updater workflow for dry-run and live execution.
//! - Runner invocation uses single-phase execution with revert-on-error enabled.

use super::super::{TaskUpdateSettings, resolve_task_update_settings};
use crate::commands::run::PhaseType;
use crate::contracts::ProjectType;
use crate::{config, prompts, runner, runutil};
use anyhow::Result;

pub(super) fn build_task_update_prompt(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
) -> Result<String> {
    let template = prompts::load_task_updater_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let prompt =
        prompts::render_task_updater_prompt(&template, task_id, project_type, &resolved.config)?;
    let prompt =
        prompts::wrap_with_repoprompt_requirement(&prompt, settings.repoprompt_tool_injection);
    prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)
}

pub(super) fn run_task_updater(
    resolved: &config::Resolved,
    settings: &TaskUpdateSettings,
    prompt: &str,
) -> Result<()> {
    let runner_settings = resolve_task_update_settings(resolved, settings)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);
    let retry_policy = runutil::RunnerRetryPolicy::from_config(&resolved.config.agent.runner_retry)
        .unwrap_or_default();

    let _output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            settings: runutil::RunnerSettings {
                repo_root: &resolved.repo_root,
                runner_kind: runner_settings.runner,
                bins,
                model: runner_settings.model.clone(),
                reasoning_effort: runner_settings.reasoning_effort,
                runner_cli: runner_settings.runner_cli,
                timeout: None,
                permission_mode: runner_settings.permission_mode,
                output_handler: None,
                output_stream: runner::OutputStream::Terminal,
            },
            execution: runutil::RunnerExecutionContext {
                prompt,
                phase_type: PhaseType::SinglePhase,
                session_id: None,
            },
            failure: runutil::RunnerFailureHandling {
                revert_on_error: true,
                git_revert_mode: resolved
                    .config
                    .agent
                    .git_revert_mode
                    .unwrap_or(crate::contracts::GitRevertMode::Ask),
                revert_prompt: None,
            },
            retry: runutil::RunnerRetryState {
                policy: retry_policy,
            },
        },
        runutil::RunnerErrorMessages {
            log_label: "task updater",
            interrupted_msg: "Task updater interrupted: agent run was canceled.",
            timeout_msg: "Task updater timed out: agent run exceeded time limit. Changes in the working tree were reverted; review repo state manually.",
            terminated_msg: "Task updater terminated: agent was stopped by a signal. Review uncommitted changes before rerunning.",
            non_zero_msg: |code| {
                format!(
                    "Task updater failed: agent exited with a non-zero code ({}). Changes in the working tree were reverted; review repo state before rerunning.",
                    code
                )
            },
            other_msg: |err| {
                format!(
                    "Task updater failed: agent could not be started or encountered an error. Error: {:#}",
                    err
                )
            },
        },
    )?;

    Ok(())
}
