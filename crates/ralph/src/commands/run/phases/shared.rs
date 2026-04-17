//! Shared helpers for run phase execution.

use super::{PhaseInvocation, PhaseType, RunExecutionTimings};
use crate::commands::run::supervision;
use crate::config;
use crate::plugins::processor_executor::ProcessorExecutor;
use crate::plugins::registry::PluginRegistry;
use crate::{runner, runutil};
use anyhow::{Context, Result};

use std::cell::RefCell;
use std::time::Instant;

pub(super) fn run_ci_gate_with_continue<F>(
    ctx: &PhaseInvocation<'_>,
    mut continue_session: supervision::ContinueSession,
    mut on_resume: F,
) -> Result<()>
where
    F: FnMut(&runner::RunnerOutput, std::time::Duration) -> Result<()>,
{
    supervision::run_ci_gate_with_continue_session(
        ctx.resolved,
        ctx.git_revert_mode,
        ctx.revert_prompt.as_ref(),
        &mut continue_session,
        |output, elapsed| on_resume(output, elapsed),
        ctx.plugins,
    )
}

/// Execute a runner pass with optional timing instrumentation.
///
/// If `execution_timings` is provided, the elapsed runner time will be recorded.
/// Invokes processor pre_prompt hooks before the runner and post_run hooks after successful
/// runner completion.
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_runner_pass(
    resolved: &config::Resolved,
    settings: &runner::AgentSettings,
    bins: runner::RunnerBinaries,
    prompt: &str,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    revert_on_error: bool,
    git_revert_mode: crate::contracts::GitRevertMode,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    log_label: &str,
    phase_type: PhaseType,
    session_id: Option<String>,
    execution_timings: Option<&RefCell<RunExecutionTimings>>,
    task_id: &str,
    plugins: Option<&PluginRegistry>,
) -> Result<runner::RunnerOutput> {
    let permission_mode = resolved.config.agent.claude_permission_mode;
    let start = Instant::now();

    // Resolve retry policy from config
    let retry_policy = runutil::RunnerRetryPolicy::from_config(&resolved.config.agent.runner_retry)
        .unwrap_or_default();

    // Apply pre_prompt hooks if plugins are available
    let final_prompt = if let Some(registry) = plugins {
        let exec = ProcessorExecutor::new(&resolved.repo_root, registry);
        exec.pre_prompt(task_id, prompt)
            .with_context(|| "processor pre_prompt hook failed")?
    } else {
        prompt.to_string()
    };

    let output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            settings: runutil::RunnerSettings {
                repo_root: &resolved.repo_root,
                runner_kind: settings.runner.clone(),
                bins,
                model: settings.model.clone(),
                reasoning_effort: settings.reasoning_effort,
                runner_cli: settings.runner_cli,
                timeout: None,
                permission_mode,
                output_handler,
                output_stream,
            },
            execution: runutil::RunnerExecutionContext {
                prompt: &final_prompt,
                phase_type,
                session_id,
            },
            failure: runutil::RunnerFailureHandling {
                revert_on_error,
                git_revert_mode,
                revert_prompt,
            },
            retry: runutil::RunnerRetryState {
                policy: retry_policy,
            },
        },
        runutil::RunnerErrorMessages {
            log_label,
            interrupted_msg: "Runner interrupted: the execution was canceled by the user or system.",
            timeout_msg: "Runner timed out: the execution exceeded the allowed time limit. Changes in the working tree were NOT reverted; review the repo state manually.",
            terminated_msg: "Runner terminated: the agent was stopped by a signal. Rerunning the task is recommended.",
            non_zero_msg: |code| {
                format!(
                    "Runner failed: the agent exited with a non-zero code ({code}). Rerunning the task is recommended after investigating the cause."
                )
            },
            other_msg: |err| {
                format!(
                    "Runner invocation failed: the agent could not be started or encountered an error. Rerunning the task is recommended. Error: {:#}",
                    err
                )
            },
        },
    )?;

    // Invoke post_run hooks after successful runner execution
    if let Some(registry) = plugins {
        let exec = ProcessorExecutor::new(&resolved.repo_root, registry);
        exec.post_run(task_id, &output.stdout)
            .with_context(|| "processor post_run hook failed")?;
    }

    if let Some(timings) = execution_timings {
        timings.borrow_mut().record_runner_duration(
            phase_type,
            &settings.runner,
            &settings.model,
            start.elapsed(),
        );
    }

    Ok(output)
}
