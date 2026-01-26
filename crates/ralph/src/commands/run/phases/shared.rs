//! Shared helpers for run phase execution.

use super::PhaseInvocation;
use crate::commands::run::supervision;
use crate::config;
use crate::{runner, runutil};
use anyhow::{bail, Result};

const CI_GATE_AUTO_RETRY_LIMIT: u8 = 2;

fn strict_ci_gate_compliance_message(
    resolved: &config::Resolved,
    _attempt: u8,
    _err: &anyhow::Error,
) -> String {
    let cmd = supervision::ci_gate_command_label(resolved);
    format!(
        r#"CI gate ({}): error: CI failed: '{}' exited with an error code. Fix the linting, type-checking, or test failures before proceeding. Compliance is mandatory. No hacky fixes allowed e.g. skipping tests, half-assed patches, etc. Implement fixes your mother would be proud of."#,
        cmd, cmd
    )
}

pub(super) fn run_ci_gate_with_continue<F>(
    ctx: &PhaseInvocation<'_>,
    mut continue_session: supervision::ContinueSession,
    mut on_resume: F,
) -> Result<()>
where
    F: FnMut(&runner::RunnerOutput) -> Result<()>,
{
    loop {
        match supervision::run_ci_gate(ctx.resolved) {
            Ok(()) => break,
            Err(err) => {
                // First two failures: bypass user prompting and auto-send a strict compliance message.
                if continue_session.ci_failure_retry_count < CI_GATE_AUTO_RETRY_LIMIT {
                    continue_session.ci_failure_retry_count =
                        continue_session.ci_failure_retry_count.saturating_add(1);
                    let attempt = continue_session.ci_failure_retry_count;

                    log::warn!(
                        "CI gate failed; auto-sending strict compliance Continue message to agent (attempt {}/{})",
                        attempt,
                        CI_GATE_AUTO_RETRY_LIMIT
                    );

                    let message = strict_ci_gate_compliance_message(ctx.resolved, attempt, &err);
                    let output = supervision::resume_continue_session(
                        ctx.resolved,
                        &mut continue_session,
                        &message,
                    )?;
                    on_resume(&output)?;
                    continue;
                }

                // 3rd+ failure: fall back to the existing revert mode behavior.
                let outcome = runutil::apply_git_revert_mode(
                    &ctx.resolved.repo_root,
                    ctx.git_revert_mode,
                    "CI failure",
                    ctx.revert_prompt.as_ref(),
                )?;

                match outcome {
                    runutil::RevertOutcome::Continue { message } => {
                        let output = supervision::resume_continue_session(
                            ctx.resolved,
                            &mut continue_session,
                            &message,
                        )?;
                        on_resume(&output)?;
                        continue;
                    }
                    _ => {
                        bail!(
                            "{} Error: {:#}",
                            runutil::format_revert_failure_message(
                                "CI gate failed after changes. Fix issues reported by CI and rerun.",
                                outcome,
                            ),
                            err
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

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
) -> Result<runner::RunnerOutput> {
    let permission_mode = resolved.config.agent.claude_permission_mode;

    runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: settings.runner,
            bins,
            model: settings.model.clone(),
            reasoning_effort: settings.reasoning_effort,
            prompt,
            timeout: None,
            permission_mode,
            revert_on_error,
            git_revert_mode,
            output_handler,
            output_stream,
            revert_prompt,
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
    )
}
