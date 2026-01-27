//! Phase 3 (review) execution and completion checks.

use super::shared::run_ci_gate_with_continue;
use super::PhaseInvocation;
use crate::commands::run::{logging, supervision};
use crate::completions;
use crate::config;
use crate::contracts::{GitRevertMode, TaskStatus};
use crate::{gitutil, promptflow, prompts, queue, runutil, timeutil};
use anyhow::{anyhow, bail, Result};

pub fn execute_phase3_review(ctx: &PhaseInvocation<'_>) -> Result<()> {
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

        let output = runutil::run_prompt_with_handling(
            runutil::RunnerInvocation {
                repo_root: &ctx.resolved.repo_root,
                runner_kind: ctx.settings.runner,
                bins: ctx.bins,
                model: ctx.settings.model.clone(),
                reasoning_effort: ctx.settings.reasoning_effort,
                runner_cli: ctx.settings.runner_cli,
                prompt: &p3_prompt,
                timeout: None,
                permission_mode: ctx.resolved.config.agent.claude_permission_mode,
                revert_on_error: false,
                git_revert_mode: ctx.git_revert_mode,
                output_handler: ctx.output_handler.clone(),
                output_stream: ctx.output_stream,
                revert_prompt: ctx.revert_prompt.clone(),
            },
            runutil::RunnerErrorMessages {
            log_label: "Code review",
            interrupted_msg: "Code review interrupted: the agent run was canceled. Review the working tree and rerun Phase 3 to complete the task.",
            timeout_msg: "Code review timed out: the agent run exceeded the time limit. Review the working tree and rerun Phase 3 to complete the task.",
            terminated_msg: "Code review terminated: the agent was stopped by a signal. Review the working tree and rerun Phase 3 to complete the task.",
                non_zero_msg: |code| {
                    format!(
                        "Code review failed: the agent exited with a non-zero code ({code}). Review the working tree and rerun Phase 3 to complete the task."
                    )
                },
                other_msg: |err| {
                    format!(
                        "Code review failed: the agent could not be started or encountered an error. Review the working tree and rerun Phase 3. Error: {:#}",
                        err
                    )
                },
            },
        )?;

        if !ctx.is_final_iteration {
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
            if completions::take_completion_signal(&ctx.resolved.repo_root, ctx.task_id)?.is_some()
            {
                log::warn!(
                    "Ignoring completion signal for {} because this run is not final.",
                    ctx.task_id
                );
            }
            return Ok(());
        }

        let mut continue_session = supervision::ContinueSession {
            runner: ctx.settings.runner,
            model: ctx.settings.model.clone(),
            reasoning_effort: ctx.settings.reasoning_effort,
            session_id: output.session_id.clone(),
            output_handler: ctx.output_handler.clone(),
            output_stream: ctx.output_stream,
            ci_failure_retry_count: 0,
        };

        let mut finalized = false;

        loop {
            let applied_status = apply_phase3_completion_signal(ctx.resolved, ctx.task_id)?;
            if !finalized
                && finalize_phase3_if_done(
                    ctx.resolved,
                    ctx.task_id,
                    applied_status,
                    ctx.git_revert_mode,
                    ctx.git_commit_push_enabled,
                    ctx.revert_prompt.clone(),
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
                            let _output = supervision::resume_continue_session(
                                ctx.resolved,
                                &mut continue_session,
                                &message,
                            )?;
                            continue;
                        }
                        _ => {
                            bail!(
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
) -> Result<Option<Phase3TaskSnapshot>> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    Ok(
        crate::commands::run::find_task_status(&queue_file, &done_file, task_id)
            .map(|(status, _title, in_done)| Phase3TaskSnapshot { status, in_done }),
    )
}

pub(crate) fn finalize_phase3_if_done(
    resolved: &config::Resolved,
    task_id: &str,
    applied_status: Option<TaskStatus>,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<bool> {
    let should_finalize = if matches!(applied_status, Some(TaskStatus::Done)) {
        true
    } else {
        load_phase3_task_snapshot(resolved, task_id)?
            .map(|snapshot| snapshot.in_done && snapshot.status == TaskStatus::Done)
            .unwrap_or(false)
    };

    if !should_finalize {
        return Ok(false);
    }

    crate::commands::run::post_run_supervise(
        resolved,
        task_id,
        git_revert_mode,
        git_commit_push_enabled,
        revert_prompt,
    )?;
    Ok(true)
}

pub fn apply_phase3_completion_signal(
    resolved: &config::Resolved,
    task_id: &str,
) -> Result<Option<TaskStatus>> {
    let Some(signal) = completions::read_completion_signal(&resolved.repo_root, task_id)? else {
        return Ok(None);
    };

    let status = signal.status;
    if let Some(snapshot) = load_phase3_task_snapshot(resolved, task_id)? {
        if snapshot.in_done {
            if snapshot.status != status {
                bail!(
                    "Completion signal status {:?} does not match archived task status {:?} for {}.",
                    status,
                    snapshot.status,
                    task_id
                );
            }
            remove_completion_signal(resolved, task_id)?;
            log::info!(
                "Completion signal for {} already applied (status {:?}); removing signal.",
                task_id,
                status
            );
            return Ok(Some(status));
        }
    }

    let now = timeutil::now_utc_rfc3339()?;
    queue::complete_task(
        &resolved.queue_path,
        &resolved.done_path,
        task_id,
        status,
        &now,
        &signal.notes,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    remove_completion_signal(resolved, task_id)?;
    log::info!(
        "Supervisor finalized task {} with status {:?} from Phase 3 completion signal.",
        task_id,
        status
    );
    Ok(Some(status))
}

fn remove_completion_signal(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let signal_path = completions::completion_signal_path(&resolved.repo_root, task_id)?;
    if let Err(err) = std::fs::remove_file(&signal_path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            return Err(err.into());
        }
    }
    Ok(())
}

pub fn ensure_phase3_completion(
    resolved: &config::Resolved,
    task_id: &str,
    git_commit_push_enabled: bool,
) -> Result<()> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
    )?;

    let (status, _title, in_done) =
        crate::commands::run::find_task_status(&queue_file, &done_file, task_id)
            .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;

    if !in_done || !(status == TaskStatus::Done || status == TaskStatus::Rejected) {
        bail!(
            "Phase 3 incomplete: task {task_id} is not archived with a terminal status. Run `ralph task done` in Phase 3 before finishing."
        );
    }

    if git_commit_push_enabled {
        if status == TaskStatus::Rejected {
            gitutil::require_clean_repo_ignoring_paths(
                &resolved.repo_root,
                false,
                gitutil::RALPH_RUN_CLEAN_ALLOWED_PATHS,
            )?;
        } else {
            gitutil::require_clean_repo_ignoring_paths(
                &resolved.repo_root,
                false,
                &[".ralph/config.json"],
            )?;
        }
    } else {
        log::info!(
            "Auto git commit/push disabled; skipping clean-repo enforcement for Phase 3 completion."
        );
    }
    Ok(())
}
