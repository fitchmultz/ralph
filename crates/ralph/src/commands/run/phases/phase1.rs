//! Phase 1 (planning) execution.
//!
//! Purpose:
//! - Phase 1 (planning) execution.
//!
//! Responsibilities:
//! - Execute Phase 1 planning runner pass and enforce plan-only output constraints.
//!
//! Not handled here:
//! - Phase 2/3 execution behavior.
//! - Queue/task selection and task status transitions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Phase 1 may only mutate files under `.ralph/`.

use super::shared::execute_runner_pass;
use super::{PhaseInvocation, PhaseType, phase_session_id_for_runner};
use crate::commands::run::{logging, supervision};
use crate::git::GitError;
use crate::{git, promptflow, prompts, runutil};
use anyhow::{Result, bail};

pub fn execute_phase1_planning(ctx: &PhaseInvocation<'_>, total_phases: u8) -> Result<String> {
    let label = logging::phase_label(1, total_phases, "Planning", ctx.task_id);

    logging::with_scope(&label, || {
        // ENFORCEMENT: Phase 1 must not implement.
        // It may only edit files under `.ralph/`.
        let allowed_paths = [".ralph/"];
        let baseline_paths = if ctx.allow_dirty_repo {
            git::status_paths(&ctx.resolved.repo_root)?
        } else {
            Vec::new()
        };
        let baseline_snapshots = if ctx.allow_dirty_repo {
            let immutable_baseline_paths: Vec<String> = baseline_paths
                .iter()
                .filter(|path| {
                    !git::clean::path_is_allowed_for_dirty_check(
                        &ctx.resolved.repo_root,
                        path,
                        &allowed_paths,
                    )
                })
                .cloned()
                .collect();
            git::snapshot_paths(&ctx.resolved.repo_root, &immutable_baseline_paths)?
        } else {
            Vec::new()
        };
        let task_refresh_instruction =
            if matches!(ctx.post_run_mode, super::PostRunMode::ParallelWorker) {
                promptflow::PHASE1_TASK_REFRESH_DISABLED_INSTRUCTION
            } else {
                promptflow::PHASE1_TASK_REFRESH_REQUIRED_INSTRUCTION
            };
        let p1_template = prompts::load_worker_phase1_prompt(&ctx.resolved.repo_root)?;
        let p1_prompt = promptflow::build_phase1_prompt(
            &p1_template,
            ctx.base_prompt,
            ctx.iteration_context,
            task_refresh_instruction,
            ctx.task_id,
            total_phases,
            ctx.policy,
            &ctx.resolved.config,
        )?;
        let phase_session_id =
            phase_session_id_for_runner(ctx.settings.runner.clone(), ctx.task_id, 1);
        let output = execute_runner_pass(
            ctx.resolved,
            ctx.settings,
            ctx.bins,
            &p1_prompt,
            ctx.output_handler.clone(),
            ctx.output_stream,
            true,
            ctx.git_revert_mode,
            ctx.revert_prompt.clone(),
            "Planning",
            PhaseType::Planning,
            phase_session_id,
            ctx.execution_timings,
            ctx.task_id,
            ctx.plugins,
        )?;

        let mut continue_session = supervision::ContinueSession {
            runner: ctx.settings.runner.clone(),
            model: ctx.settings.model.clone(),
            reasoning_effort: ctx.settings.reasoning_effort,
            runner_cli: ctx.settings.runner_cli,
            phase_type: super::PhaseType::Planning,
            session_id: output.session_id.clone(),
            output_handler: ctx.output_handler.clone(),
            output_stream: ctx.output_stream,
            run_event_handler: ctx.run_event_handler.clone(),
            ci_failure_retry_count: 0,
            task_id: ctx.task_id.to_string(),
            last_ci_error_pattern: None,
            consecutive_same_error_count: 0,
        };

        loop {
            let mut allowed: Vec<String> = allowed_paths
                .iter()
                .map(|value| value.to_string())
                .collect();
            allowed.extend(baseline_paths.iter().cloned());
            let allowed_refs: Vec<&str> = allowed.iter().map(String::as_str).collect();

            let status = if ctx.is_followup_iteration {
                let current = git::status_paths(&ctx.resolved.repo_root)?;
                if current.is_empty()
                    || git::repo_dirty_only_allowed_paths(&ctx.resolved.repo_root, &allowed_refs)?
                {
                    Ok(())
                } else {
                    Err(GitError::DirtyRepo {
                        details: "\n\nFollow-up Phase 1 violation: planning introduced dirty paths outside baseline and allowed .ralph paths."
                            .to_string(),
                    })
                }
            } else {
                git::require_clean_repo_ignoring_paths(
                    &ctx.resolved.repo_root,
                    false,
                    &allowed_refs,
                )
            };

            let snapshot_check = match status {
                Ok(()) => git::ensure_paths_unchanged(&ctx.resolved.repo_root, &baseline_snapshots)
                    .map_err(|err| GitError::Other(err.context("baseline dirty path changed"))),
                Err(err) => Err(err),
            };

            match snapshot_check {
                Ok(()) => break,
                Err(err) => {
                    let outcome = runutil::apply_git_revert_mode_with_context(
                        &ctx.resolved.repo_root,
                        ctx.git_revert_mode,
                        runutil::RevertPromptContext::new("Phase 1 plan-only violation", true),
                        ctx.revert_prompt.as_ref(),
                    )?;
                    match outcome {
                        runutil::RevertOutcome::Continue { message } => {
                            let resumed = supervision::resume_continue_session(
                                ctx.resolved,
                                &mut continue_session,
                                &message,
                                ctx.plugins,
                            )?;
                            let elapsed = resumed.elapsed;
                            // Record resume duration for Phase 1
                            if let Some(timings) = ctx.execution_timings {
                                timings.borrow_mut().record_runner_duration(
                                    PhaseType::Planning,
                                    &continue_session.runner,
                                    &continue_session.model,
                                    elapsed,
                                );
                            }
                            continue;
                        }
                        runutil::RevertOutcome::Proceed { reason } => {
                            log::warn!(
                                "Phase 1 plan-only violation override: proceeding without reverting ({reason})."
                            );
                            break;
                        }
                        _ => {
                            bail!(
                                "{} Error: {:#}",
                                runutil::format_revert_failure_message(
                                    "Phase 1 violated plan-only contract: it modified files outside allowed .ralph paths, including baseline dirty paths.",
                                    outcome,
                                ),
                                err
                            );
                        }
                    }
                }
            }
        }

        // Read plan from cache (Phase 1 writes it directly).
        let plan_text = promptflow::read_plan_cache(&ctx.resolved.repo_root, ctx.task_id)?;
        log::info!(
            "Plan cached for {} at {}",
            ctx.task_id,
            promptflow::plan_cache_path(&ctx.resolved.repo_root, ctx.task_id).display()
        );

        Ok(plan_text)
    })
}
