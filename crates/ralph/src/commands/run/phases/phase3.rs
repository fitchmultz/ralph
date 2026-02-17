//! Phase 3 (review) execution and completion checks.

use super::shared::{execute_runner_pass, run_ci_gate_with_continue};
use super::{PhaseInvocation, PhaseType, PostRunMode, phase_session_id_for_runner};
use crate::commands::run::{logging, supervision};
use crate::completions;
use crate::config;
use crate::constants::custom_fields::{MODEL_USED, RUNNER_USED};
use crate::contracts::{GitRevertMode, TaskStatus};
use crate::{git, promptflow, prompts, queue, runner, runutil, timeutil};
use anyhow::{Result, anyhow, bail};
use std::collections::HashMap;

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
            let applied_status = apply_phase3_completion_signal(ctx.resolved, ctx.task_id)?;
            if !finalized
                && finalize_phase3_if_done(
                    ctx.resolved,
                    ctx.task_id,
                    applied_status,
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
    applied_status: Option<TaskStatus>,
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

pub fn apply_phase3_completion_signal(
    resolved: &config::Resolved,
    task_id: &str,
) -> Result<Option<TaskStatus>> {
    let Some(signal) = completions::read_completion_signal(&resolved.repo_root, task_id)? else {
        return Ok(None);
    };

    let status = signal.status;
    if let Some(snapshot) = load_phase3_task_snapshot(resolved, task_id)?
        && snapshot.in_done
    {
        if snapshot.status != status {
            bail!(
                "Completion signal status {:?} does not match archived task status {:?} for {}.",
                status,
                snapshot.status,
                task_id
            );
        }

        // Apply any missing custom_fields from the signal to the already-archived task.
        // If patching fails, keep the signal so we can retry later rather than losing analytics data.
        if let Some(custom_fields_patch) = build_custom_fields_patch_from_signal(&signal) {
            patch_done_task_custom_fields(resolved, task_id, &custom_fields_patch)?;
        }

        remove_completion_signal(resolved, task_id)?;
        log::info!(
            "Completion signal for {} already applied (status {:?}); removing signal.",
            task_id,
            status
        );
        return Ok(Some(status));
    }

    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    // Build custom fields patch from completion signal for observational analytics
    let custom_fields_patch = build_custom_fields_patch_from_signal(&signal);

    queue::complete_task(
        &resolved.queue_path,
        &resolved.done_path,
        task_id,
        status,
        &now,
        &signal.notes,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
        custom_fields_patch.as_ref(),
    )?;
    remove_completion_signal(resolved, task_id)?;
    log::info!(
        "Supervisor finalized task {} with status {:?} from Phase 3 completion signal.",
        task_id,
        status
    );
    Ok(Some(status))
}

/// Patch custom fields into an already-archived task in done.json.
fn patch_done_task_custom_fields(
    resolved: &config::Resolved,
    task_id: &str,
    patch: &HashMap<String, String>,
) -> Result<()> {
    let mut done = queue::load_queue_or_default(&resolved.done_path)?;

    let Some(task) = done
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == task_id.trim())
    else {
        bail!(
            "{}",
            crate::error_messages::task_not_found_in_done_archive(task_id, "custom_fields patch")
        );
    };

    let mut modified = false;
    for (k, v) in patch {
        let key = k.trim();
        let val = v.trim();
        if key.is_empty() || val.is_empty() {
            continue;
        }
        // Only insert if not already present (observation wins if already set)
        task.custom_fields
            .entry(key.to_string())
            .or_insert_with(|| {
                modified = true;
                val.to_string()
            });
    }
    if modified {
        queue::save_queue(&resolved.done_path, &done)?;
        log::info!("Patched custom fields for {} in done.json", task_id);
    }

    Ok(())
}

/// Build custom fields patch from completion signal.
fn build_custom_fields_patch_from_signal(
    signal: &completions::CompletionSignal,
) -> Option<HashMap<String, String>> {
    let mut patch = HashMap::new();

    if let Some(ref runner) = signal.runner_used {
        let trimmed = runner.trim();
        if !trimmed.is_empty() {
            patch.insert(RUNNER_USED.to_string(), trimmed.to_ascii_lowercase());
        }
    }
    if let Some(ref model) = signal.model_used {
        let trimmed = model.trim();
        if !trimmed.is_empty() {
            patch.insert(MODEL_USED.to_string(), trimmed.to_string());
        }
    }

    if patch.is_empty() { None } else { Some(patch) }
}

fn remove_completion_signal(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let signal_path = completions::completion_signal_path(&resolved.repo_root, task_id)?;
    if let Err(err) = std::fs::remove_file(&signal_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        return Err(err.into());
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
            anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue_or_done(task_id)
            )
        })?;

    if !in_done || !(status == TaskStatus::Done || status == TaskStatus::Rejected) {
        bail!(
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
