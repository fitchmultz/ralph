//! Phase 2 (implementation) execution.

use super::shared::{execute_runner_pass, run_ci_gate_with_continue};
use super::{PhaseInvocation, PhaseType};
use crate::commands::run::{logging, supervision};
use crate::{promptflow, prompts, runner};
use anyhow::Result;
use std::path::Path;

pub(super) const PHASE2_FINAL_RESPONSE_FALLBACK: &str = "(Phase 2 final response unavailable.)";

pub(super) fn cache_phase2_final_response(
    repo_root: &Path,
    task_id: &str,
    stdout: &str,
) -> Result<()> {
    let phase2_final_response = runner::extract_final_assistant_response(stdout)
        .unwrap_or_else(|| PHASE2_FINAL_RESPONSE_FALLBACK.to_string());
    promptflow::write_phase2_final_response_cache(repo_root, task_id, &phase2_final_response)
}

pub fn execute_phase2_implementation(
    ctx: &PhaseInvocation<'_>,
    total_phases: u8,
    plan_text: &str,
) -> Result<()> {
    let label = logging::phase_label(2, total_phases, "Implementation", ctx.task_id);

    logging::with_scope(&label, || {
        if total_phases == 3 {
            let handoff_template = prompts::load_phase2_handoff_checklist(&ctx.resolved.repo_root)?;
            let handoff_checklist =
                prompts::render_phase2_handoff_checklist(&handoff_template, &ctx.resolved.config)?;
            let p2_template = prompts::load_worker_phase2_handoff_prompt(&ctx.resolved.repo_root)?;
            let p2_prompt = promptflow::build_phase2_handoff_prompt(
                &p2_template,
                ctx.base_prompt,
                plan_text,
                &handoff_checklist,
                ctx.iteration_context,
                ctx.iteration_completion_block,
                ctx.task_id,
                total_phases,
                ctx.policy,
                &ctx.resolved.config,
            )?;

            let output = execute_runner_pass(
                ctx.resolved,
                ctx.settings,
                ctx.bins,
                &p2_prompt,
                ctx.output_handler.clone(),
                ctx.output_stream,
                true,
                ctx.git_revert_mode,
                ctx.revert_prompt.clone(),
                "Implementation",
                PhaseType::Implementation,
            )?;

            cache_phase2_final_response(&ctx.resolved.repo_root, ctx.task_id, &output.stdout)?;

            let continue_session = supervision::ContinueSession {
                runner: ctx.settings.runner,
                model: ctx.settings.model.clone(),
                reasoning_effort: ctx.settings.reasoning_effort,
                session_id: output.session_id.clone(),
                output_handler: ctx.output_handler.clone(),
                output_stream: ctx.output_stream,
                ci_failure_retry_count: 0,
            };

            run_ci_gate_with_continue(ctx, continue_session, |output| {
                cache_phase2_final_response(&ctx.resolved.repo_root, ctx.task_id, &output.stdout)
            })?;

            return Ok(());
        }

        let checklist = if ctx.is_final_iteration {
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
        let p2_template = prompts::load_worker_phase2_prompt(&ctx.resolved.repo_root)?;
        let p2_prompt = promptflow::build_phase2_prompt(
            &p2_template,
            ctx.base_prompt,
            plan_text,
            &checklist,
            ctx.iteration_context,
            ctx.iteration_completion_block,
            ctx.task_id,
            total_phases,
            ctx.policy,
            &ctx.resolved.config,
        )?;

        let output = execute_runner_pass(
            ctx.resolved,
            ctx.settings,
            ctx.bins,
            &p2_prompt,
            ctx.output_handler.clone(),
            ctx.output_stream,
            true,
            ctx.git_revert_mode,
            ctx.revert_prompt.clone(),
            "Implementation",
            PhaseType::Implementation,
        )?;

        if ctx.is_final_iteration {
            crate::commands::run::post_run_supervise(
                ctx.resolved,
                ctx.task_id,
                ctx.git_revert_mode,
                ctx.git_commit_push_enabled,
                ctx.revert_prompt.clone(),
                ctx.notify_on_complete,
                ctx.notify_sound,
            )?;
        } else {
            let continue_session = supervision::ContinueSession {
                runner: ctx.settings.runner,
                model: ctx.settings.model.clone(),
                reasoning_effort: ctx.settings.reasoning_effort,
                session_id: output.session_id.clone(),
                output_handler: ctx.output_handler.clone(),
                output_stream: ctx.output_stream,
                ci_failure_retry_count: 0,
            };
            run_ci_gate_with_continue(ctx, continue_session, |_output| Ok(()))?;
        }
        Ok(())
    })
}
