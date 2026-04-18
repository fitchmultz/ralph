//! Purpose: Handle final-iteration Phase 3 completion flows.
//!
//! Responsibilities:
//! - Route final review completion to either parallel integration or normal supervision.
//! - Run the Phase 3 finalization loop until task archival/completion checks pass.
//! - Preserve review resume-duration accounting across continuation paths.
//!
//! Scope:
//! - Final-iteration control flow only.
//! - Snapshot/completion validation helpers live in `completion.rs`.
//!
//! Usage:
//! - Called by `phase3/mod.rs` after the final review runner pass succeeds.
//!
//! Invariants/Assumptions:
//! - Parallel workers require an explicit coordinator-selected target branch.
//! - Normal finalization keeps retrying only through the canonical continue-session path.

use anyhow::{Result, anyhow, bail};

use super::super::{PhaseInvocation, PostRunMode};
use super::completion::{ensure_phase3_completion, finalize_phase3_if_done};
use crate::commands::run::supervision;
use crate::{runner, runutil};

pub(super) fn complete_final_review(
    ctx: &PhaseInvocation<'_>,
    session_id: Option<String>,
) -> Result<()> {
    let mut continue_session = super::build_continue_session(ctx, session_id);
    match ctx.post_run_mode {
        PostRunMode::Normal => run_finalization_loop(ctx, &mut continue_session),
        PostRunMode::ParallelWorker => run_parallel_integration(ctx, &mut continue_session),
    }
}

fn run_parallel_integration(
    ctx: &PhaseInvocation<'_>,
    continue_session: &mut supervision::ContinueSession,
) -> Result<()> {
    use crate::commands::run::parallel::{
        IntegrationConfig, IntegrationOutcome, run_integration_loop,
    };

    let target_branch = ctx
        .parallel_target_branch
        .ok_or_else(|| anyhow!("parallel worker integration requires explicit target branch"))?;
    let config = IntegrationConfig::from_resolved(ctx.resolved, target_branch);
    let task_title = ctx.task_title.unwrap_or(ctx.task_id);
    let phase_summary = format!("Completed phases 1-3 for {}", ctx.task_id);
    let mut on_resume = |_resume_output: &runner::RunnerOutput, elapsed: std::time::Duration| {
        super::record_phase3_duration(ctx, elapsed);
        Ok(())
    };

    match run_integration_loop(
        ctx.resolved,
        ctx.task_id,
        task_title,
        &config,
        &phase_summary,
        continue_session,
        &mut on_resume,
        ctx.plugins,
    )? {
        IntegrationOutcome::Success => {
            log::info!("Integration loop succeeded for {}", ctx.task_id);
            Ok(())
        }
        IntegrationOutcome::BlockedPush { reason } => {
            log::warn!("Integration loop blocked for {}: {}", ctx.task_id, reason);
            bail!("Push blocked: {}", reason)
        }
        IntegrationOutcome::Failed { reason } => {
            log::error!("Integration loop failed for {}: {}", ctx.task_id, reason);
            bail!("Integration failed: {}", reason)
        }
    }
}

fn run_finalization_loop(
    ctx: &PhaseInvocation<'_>,
    continue_session: &mut supervision::ContinueSession,
) -> Result<()> {
    let mut finalized = false;
    let mut on_resume = |_resume_output: &runner::RunnerOutput, elapsed: std::time::Duration| {
        super::record_phase3_duration(ctx, elapsed);
        Ok(())
    };

    loop {
        if !finalized
            && finalize_phase3_if_done(
                ctx.resolved,
                ctx.queue_lock,
                ctx.task_id,
                ctx.git_revert_mode,
                ctx.git_publish_mode,
                ctx.push_policy,
                ctx.revert_prompt.clone(),
                Some(supervision::CiContinueContext {
                    continue_session,
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

        match ensure_phase3_completion(ctx.resolved, ctx.task_id, ctx.git_publish_mode) {
            Ok(()) => return Ok(()),
            Err(err) => {
                let outcome = runutil::apply_git_revert_mode(
                    &ctx.resolved.repo_root,
                    ctx.git_revert_mode,
                    "Phase 3 completion check",
                    ctx.revert_prompt.as_ref(),
                )?;
                match outcome {
                    runutil::RevertOutcome::Continue { message } => {
                        let resumed = supervision::resume_continue_session(
                            ctx.resolved,
                            continue_session,
                            &message,
                            ctx.plugins,
                        )?;
                        super::record_phase3_duration(ctx, resumed.elapsed);
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
}
