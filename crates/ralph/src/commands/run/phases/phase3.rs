//! Phase 3 (review) execution and completion checks.

use super::shared::{execute_runner_pass, run_ci_gate_with_continue};
use super::{PhaseInvocation, PhaseType, PostRunMode, phase_session_id_for_runner};
use crate::commands::run::{logging, supervision};
use crate::config;
use crate::contracts::{GitRevertMode, TaskStatus};
use crate::{git, promptflow, prompts, queue, runner, runutil};

pub fn execute_phase3_review(ctx: &PhaseInvocation<'_>) -> Result<(), anyhow::Error> {
    let label = logging::phase_label(3, 3, "Review", ctx.task_id);

    logging::with_scope(&label, || {
        let review_template = prompts::load_code_review_prompt(&ctx.resolved.repo_root)?;
        let review_body = prompts::render_code_review_prompt(
            &review_template,
            ctx.task_id,
            ctx.project_type,
            &ctx.resolved.config,
        )?;

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
        let p3_template = prompts::load_worker_phase3_prompt(&ctx.resolved.repo_root)?;
        let phase2_final_response = match promptflow::read_phase2_final_response_cache(
            &ctx.resolved.repo_root,
            ctx.task_id,
        ) {
            Ok(text) => text,
            Err(err) => {
                log::warn!(
                    "Phase 2 final response cache unavailable for {}: {}",
                    ctx.task_id,
                    err
                );
                "(Phase 2 final response unavailable; cache missing.)".to_string()
            }
        };
        let p3_prompt = promptflow::build_phase3_prompt(
            &p3_template,
            ctx.base_prompt,
            &review_body,
            &phase2_final_response,
            ctx.task_id,
            &completion_checklist,
            ctx.iteration_context,
            ctx.iteration_completion_block,
            ctx.phase3_completion_guidance,
            3,
            ctx.policy,
            &ctx.resolved.config,
        )?;

        let phase_session_id =
            phase_session_id_for_runner(ctx.settings.runner.clone(), ctx.task_id, 3);
        let output = execute_runner_pass(
            ctx.resolved,
            ctx.settings,
            ctx.bins,
            &p3_prompt,
            ctx.output_handler.clone(),
            ctx.output_stream,
            false, // Phase 3 does not revert on error
            ctx.git_revert_mode,
            ctx.revert_prompt.clone(),
            "Code review",
            PhaseType::Review,
            phase_session_id,
            ctx.execution_timings,
            ctx.task_id,
            ctx.plugins,
        )?;

        if !ctx.is_final_iteration {
            let continue_session = supervision::ContinueSession {
                runner: ctx.settings.runner.clone(),
                model: ctx.settings.model.clone(),
                reasoning_effort: ctx.settings.reasoning_effort,
                runner_cli: ctx.settings.runner_cli,
                phase_type: super::PhaseType::Review,
                session_id: output.session_id.clone(),
                output_handler: ctx.output_handler.clone(),
                output_stream: ctx.output_stream,
                ci_failure_retry_count: 0,
                task_id: ctx.task_id.to_string(),
            };
            let timings = ctx.execution_timings;
            let runner = ctx.settings.runner.clone();
            let model = ctx.settings.model.clone();
            run_ci_gate_with_continue(ctx, continue_session, |_output, elapsed| {
                if let Some(timings) = timings {
                    timings.borrow_mut().record_runner_duration(
                        PhaseType::Review,
                        &runner,
                        &model,
                        elapsed,
                    );
                }
                Ok(())
            })?;
            return Ok(());
        }

        let mut continue_session = supervision::ContinueSession {
            runner: ctx.settings.runner.clone(),
            model: ctx.settings.model.clone(),
            reasoning_effort: ctx.settings.reasoning_effort,
            runner_cli: ctx.settings.runner_cli,
            phase_type: super::PhaseType::Review,
            session_id: output.session_id.clone(),
            output_handler: ctx.output_handler.clone(),
            output_stream: ctx.output_stream,
            ci_failure_retry_count: 0,
            task_id: ctx.task_id.to_string(),
        };

        if ctx.post_run_mode == PostRunMode::ParallelWorker {
            let _runner = ctx.settings.runner.clone();
            let _model = ctx.settings.model.clone();
            let mut on_resume =
                |_resume_output: &runner::RunnerOutput, _elapsed: std::time::Duration| Ok(());
            crate::commands::run::post_run_supervise_parallel_worker(
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
                ctx.lfs_check,
                ctx.plugins,
            )?;
            return Ok(());
        }

        let mut finalized = false;
        let runner = ctx.settings.runner.clone();
        let model = ctx.settings.model.clone();
        let timings = ctx.execution_timings;
        let mut on_resume = move |_resume_output: &runner::RunnerOutput,
                                  elapsed: std::time::Duration| {
            // Record resume duration for Phase 3
            if let Some(timings) = timings {
                timings.borrow_mut().record_runner_duration(
                    PhaseType::Review,
                    &runner,
                    &model,
                    elapsed,
                );
            }
            Ok(())
        };

        loop {
            if !finalized
                && finalize_phase3_if_done(
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
                )?
            {
                finalized = true;
            }

            match ensure_phase3_completion(ctx.resolved, ctx.task_id, ctx.git_commit_push_enabled) {
                Ok(()) => break,
                Err(err) => {
                    let outcome = runutil::apply_git_revert_mode(
                        &ctx.resolved.repo_root,
                        ctx.git_revert_mode,
                        "Phase 3 completion check",
                        ctx.revert_prompt.as_ref(),
                    )?;
                    match outcome {
                        runutil::RevertOutcome::Continue { message } => {
                            let (_output, elapsed) = supervision::resume_continue_session(
                                ctx.resolved,
                                &mut continue_session,
                                &message,
                                ctx.plugins,
                            )?;
                            // Record resume duration for Phase 3
                            if let Some(timings) = ctx.execution_timings {
                                timings.borrow_mut().record_runner_duration(
                                    PhaseType::Review,
                                    &continue_session.runner,
                                    &continue_session.model,
                                    elapsed,
                                );
                            }
                            continue;
                        }
                        _ => {
                            anyhow::bail!(
                                "{} Error: {:#}",
                                runutil::format_revert_failure_message(
                                    "Phase 3 incomplete: task was not archived with a terminal status.",
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
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Phase3TaskSnapshot {
    status: TaskStatus,
    in_done: bool,
}

fn load_phase3_task_snapshot(
    resolved: &config::Resolved,
    task_id: &str,
) -> Result<Option<Phase3TaskSnapshot>, anyhow::Error> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    Ok(
        supervision::find_task_status(&queue_file, &done_file, task_id)
            .map(|(status, _title, in_done)| Phase3TaskSnapshot { status, in_done }),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finalize_phase3_if_done(
    resolved: &config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: crate::commands::run::supervision::PushPolicy,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    ci_continue: Option<supervision::CiContinueContext<'_>>,
    notify_on_complete: Option<bool>,
    notify_sound: Option<bool>,
    lfs_check: bool,
    no_progress: bool,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<bool, anyhow::Error> {
    let should_finalize = load_phase3_task_snapshot(resolved, task_id)?
        .map(|snapshot| snapshot.in_done && snapshot.status == TaskStatus::Done)
        .unwrap_or(false);

    if !should_finalize {
        return Ok(false);
    }

    crate::commands::run::post_run_supervise(
        resolved,
        task_id,
        git_revert_mode,
        git_commit_push_enabled,
        push_policy,
        revert_prompt,
        ci_continue,
        notify_on_complete,
        notify_sound,
        lfs_check,
        no_progress,
        plugins,
    )?;
    Ok(true)
}

pub fn ensure_phase3_completion(
    resolved: &config::Resolved,
    task_id: &str,
    git_commit_push_enabled: bool,
) -> Result<(), anyhow::Error> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    let (status, _title, in_done) = supervision::find_task_status(&queue_file, &done_file, task_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue_or_done(task_id)
            )
        })?;

    if !in_done || !(status == TaskStatus::Done || status == TaskStatus::Rejected) {
        anyhow::bail!(
            "Phase 3 incomplete: task {task_id} is not archived with a terminal status. Run `ralph task done` in Phase 3 before finishing."
        );
    }

    if git_commit_push_enabled {
        if status == TaskStatus::Rejected {
            git::require_clean_repo_ignoring_paths(
                &resolved.repo_root,
                false,
                git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
            )?;
        } else {
            git::require_clean_repo_ignoring_paths(
                &resolved.repo_root,
                false,
                &[
                    ".ralph/config.json",
                    ".ralph/config.jsonc",
                    ".ralph/cache/productivity.json",
                ],
            )?;
        }
    } else {
        log::info!(
            "Auto git commit/push disabled; skipping clean-repo enforcement for Phase 3 completion."
        );
    }
    Ok(())
}
