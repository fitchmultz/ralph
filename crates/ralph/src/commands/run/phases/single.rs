//! Single-phase execution.

use super::shared::{execute_runner_pass, run_ci_gate_with_continue};
use super::{PhaseInvocation, PhaseType, PostRunMode, phase_session_id_for_runner};
use crate::commands::run::{logging, supervision};
use crate::{promptflow, prompts, runner};
use anyhow::Result;

pub fn execute_single_phase(ctx: &PhaseInvocation<'_>) -> Result<()> {
    let label = logging::single_phase_label("SinglePhase (Execution)", ctx.task_id);

    logging::with_scope(&label, || {
        let completion_checklist = if ctx.is_final_iteration {
            let checklist_template = prompts::load_completion_checklist(&ctx.resolved.repo_root)?;
            prompts::render_completion_checklist(
                &checklist_template,
                ctx.task_id,
                &ctx.resolved.config,
            )?
        } else {
            let checklist_template = prompts::load_iteration_checklist(&ctx.resolved.repo_root)?;
            prompts::render_iteration_checklist(
                &checklist_template,
                ctx.task_id,
                &ctx.resolved.config,
            )?
        };
        let single_template = prompts::load_worker_single_phase_prompt(&ctx.resolved.repo_root)?;
        let prompt = promptflow::build_single_phase_prompt(
            &single_template,
            ctx.base_prompt,
            &completion_checklist,
            ctx.iteration_context,
            ctx.iteration_completion_block,
            ctx.task_id,
            ctx.policy,
            &ctx.resolved.config,
        )?;

        let phase_session_id =
            phase_session_id_for_runner(ctx.settings.runner.clone(), ctx.task_id, 0);
        let output = execute_runner_pass(
            ctx.resolved,
            ctx.settings,
            ctx.bins,
            &prompt,
            ctx.output_handler.clone(),
            ctx.output_stream,
            true,
            ctx.git_revert_mode,
            ctx.revert_prompt.clone(),
            "Execution",
            PhaseType::SinglePhase,
            phase_session_id,
            ctx.execution_timings,
            ctx.task_id,
            ctx.plugins,
        )?;

        if ctx.is_final_iteration {
            let mut continue_session = supervision::ContinueSession {
                runner: ctx.settings.runner.clone(),
                model: ctx.settings.model.clone(),
                reasoning_effort: ctx.settings.reasoning_effort,
                runner_cli: ctx.settings.runner_cli,
                phase_type: super::PhaseType::SinglePhase,
                session_id: output.session_id.clone(),
                output_handler: ctx.output_handler.clone(),
                output_stream: ctx.output_stream,
                ci_failure_retry_count: 0,
                task_id: ctx.task_id.to_string(),
                last_ci_error_pattern: None,
                consecutive_same_error_count: 0,
            };
            let runner = ctx.settings.runner.clone();
            let model = ctx.settings.model.clone();
            let timings = ctx.execution_timings;
            let mut on_resume =
                move |_resume_output: &runner::RunnerOutput, elapsed: std::time::Duration| {
                    // Record resume duration for single phase
                    if let Some(timings) = timings {
                        timings.borrow_mut().record_runner_duration(
                            PhaseType::SinglePhase,
                            &runner,
                            &model,
                            elapsed,
                        );
                    }
                    Ok(())
                };
            match ctx.post_run_mode {
                PostRunMode::Normal => crate::commands::run::post_run_supervise(
                    ctx.resolved,
                    ctx.task_id,
                    ctx.git_revert_mode,
                    ctx.git_commit_push_enabled,
                    ctx.push_policy,
                    ctx.revert_prompt.clone(),
                    Some(supervision::CiContinueContext {
                        continue_session: &mut continue_session,
                        on_resume: &mut on_resume,
                    }),
                    ctx.notify_on_complete,
                    ctx.notify_sound,
                    ctx.lfs_check,
                    ctx.no_progress,
                    ctx.plugins,
                )?,
                PostRunMode::ParallelWorker => {
                    use crate::commands::run::parallel::{IntegrationConfig, run_integration_loop};

                    let config = IntegrationConfig::from_resolved(ctx.resolved);
                    let task_title = ctx.task_title.unwrap_or(ctx.task_id);
                    let phase_summary = format!("Completed single phase for {}", ctx.task_id);
                    let integration_runner = ctx.settings.runner.clone();
                    let integration_model = ctx.settings.model.clone();
                    let integration_timings = ctx.execution_timings;
                    let mut integration_on_resume =
                        move |resume_output: &runner::RunnerOutput,
                              elapsed: std::time::Duration| {
                            if let Some(timings) = integration_timings {
                                timings.borrow_mut().record_runner_duration(
                                    PhaseType::SinglePhase,
                                    &integration_runner,
                                    &integration_model,
                                    elapsed,
                                );
                            }
                            let _ = resume_output;
                            Ok(())
                        };

                    match run_integration_loop(
                        ctx.resolved,
                        ctx.task_id,
                        task_title,
                        &config,
                        &phase_summary,
                        &mut continue_session,
                        &mut integration_on_resume,
                        ctx.plugins,
                    ) {
                        Ok(crate::commands::run::parallel::IntegrationOutcome::Success) => {
                            log::info!("Integration loop succeeded for {}", ctx.task_id);
                        }
                        Ok(crate::commands::run::parallel::IntegrationOutcome::BlockedPush {
                            reason,
                        }) => {
                            log::warn!("Integration loop blocked for {}: {}", ctx.task_id, reason);
                            anyhow::bail!("Push blocked: {}", reason);
                        }
                        Ok(crate::commands::run::parallel::IntegrationOutcome::Failed {
                            reason,
                        }) => {
                            log::error!("Integration loop failed for {}: {}", ctx.task_id, reason);
                            anyhow::bail!("Integration failed: {}", reason);
                        }
                        Err(e) => {
                            log::error!("Integration loop error for {}: {}", ctx.task_id, e);
                            anyhow::bail!("Integration error: {}", e);
                        }
                    }
                }
            }
        } else {
            let continue_session = supervision::ContinueSession {
                runner: ctx.settings.runner.clone(),
                model: ctx.settings.model.clone(),
                reasoning_effort: ctx.settings.reasoning_effort,
                runner_cli: ctx.settings.runner_cli,
                phase_type: super::PhaseType::SinglePhase,
                session_id: output.session_id.clone(),
                output_handler: ctx.output_handler.clone(),
                output_stream: ctx.output_stream,
                ci_failure_retry_count: 0,
                task_id: ctx.task_id.to_string(),
                last_ci_error_pattern: None,
                consecutive_same_error_count: 0,
            };
            let timings = ctx.execution_timings;
            let runner = ctx.settings.runner.clone();
            let model = ctx.settings.model.clone();
            run_ci_gate_with_continue(ctx, continue_session, |_output, elapsed| {
                if let Some(timings) = timings {
                    timings.borrow_mut().record_runner_duration(
                        PhaseType::SinglePhase,
                        &runner,
                        &model,
                        elapsed,
                    );
                }
                Ok(())
            })?;
        }
        Ok(())
    })
}
