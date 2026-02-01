//! Phase 1 (planning) execution.

use super::shared::execute_runner_pass;
use super::{PhaseInvocation, PhaseType};
use crate::commands::run::{logging, supervision};
use crate::git::GitError;
use crate::{git, promptflow, prompts, runutil};
use anyhow::{Result, bail};

pub fn execute_phase1_planning(ctx: &PhaseInvocation<'_>, total_phases: u8) -> Result<String> {
    let label = logging::phase_label(1, total_phases, "Planning", ctx.task_id);

    logging::with_scope(&label, || {
        let baseline_paths = if ctx.allow_dirty_repo {
            git::status_paths(&ctx.resolved.repo_root)?
        } else {
            Vec::new()
        };
        let baseline_snapshots = if ctx.allow_dirty_repo {
            git::snapshot_paths(&ctx.resolved.repo_root, &baseline_paths)?
        } else {
            Vec::new()
        };
        let p1_template = prompts::load_worker_phase1_prompt(&ctx.resolved.repo_root)?;
        let p1_prompt = promptflow::build_phase1_prompt(
            &p1_template,
            ctx.base_prompt,
            ctx.iteration_context,
            ctx.task_id,
            total_phases,
            ctx.policy,
            &ctx.resolved.config,
        )?;
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
        )?;

        let mut continue_session = supervision::ContinueSession {
            runner: ctx.settings.runner,
            model: ctx.settings.model.clone(),
            reasoning_effort: ctx.settings.reasoning_effort,
            runner_cli: ctx.settings.runner_cli,
            phase_type: super::PhaseType::Planning,
            session_id: output.session_id.clone(),
            output_handler: ctx.output_handler.clone(),
            output_stream: ctx.output_stream,
            ci_failure_retry_count: 0,
        };

        // ENFORCEMENT: Phase 1 must not implement.
        // It may only edit `.ralph/queue.json` / `.ralph/done.json` (status bookkeeping)
        // plus the plan cache file for the current task.
        let plan_cache_rel = format!(".ralph/cache/plans/{}.md", ctx.task_id);
        let plan_cache_dir = ".ralph/cache/plans/";
        let allowed_paths = [
            ".ralph/queue.json",
            ".ralph/done.json",
            plan_cache_rel.as_str(),
            plan_cache_dir,
        ];
        loop {
            let mut allowed: Vec<String> = allowed_paths
                .iter()
                .map(|value| value.to_string())
                .collect();
            allowed.extend(baseline_paths.iter().cloned());
            let allowed_refs: Vec<&str> = allowed.iter().map(String::as_str).collect();

            let status = git::require_clean_repo_ignoring_paths(
                &ctx.resolved.repo_root,
                false,
                &allowed_refs,
            );
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
                            let _output = supervision::resume_continue_session(
                                ctx.resolved,
                                &mut continue_session,
                                &message,
                            )?;
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
                                    "Phase 1 violated plan-only contract: it modified files outside allowed queue bookkeeping, including baseline dirty paths.",
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
