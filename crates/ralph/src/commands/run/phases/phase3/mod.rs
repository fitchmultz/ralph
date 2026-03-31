//! Purpose: Phase 3 review orchestration facade.
//!
//! Responsibilities:
//! - Run the Phase 3 review prompt through the shared phase runner.
//! - Delegate prompt assembly, non-final CI flow, and finalization/completion checks.
//! - Re-export the completion helpers used by runtime tests and sibling run modules.
//!
//! Scope:
//! - Thin orchestration only; detailed prompt/finalization logic lives in companion modules.
//! - Shared runner execution and CI helpers remain in `phases/shared.rs`.
//!
//! Usage:
//! - Imported by `commands/run/phases/mod.rs` as the Phase 3 entrypoint.
//!
//! Invariants/Assumptions:
//! - Phase 3 never enables runner auto-revert on the initial review pass.
//! - Resume-duration accounting for review stays aligned across CI, integration, and completion flows.

use std::time::Duration;

use anyhow::Result;

use super::shared::execute_runner_pass;
use super::{PhaseInvocation, PhaseType, phase_session_id_for_runner};
use crate::commands::run::{logging, supervision};

mod completion;
mod finalization;
mod non_final;
mod prompt;

#[cfg(test)]
pub(crate) use completion::ensure_phase3_completion;

pub fn execute_phase3_review(ctx: &PhaseInvocation<'_>) -> Result<()> {
    let label = logging::phase_label(3, 3, "Review", ctx.task_id);

    logging::with_scope(&label, || {
        let prompt = prompt::build_review_prompt(ctx)?;
        let phase_session_id =
            phase_session_id_for_runner(ctx.settings.runner.clone(), ctx.task_id, 3);
        let output = execute_runner_pass(
            ctx.resolved,
            ctx.settings,
            ctx.bins,
            &prompt,
            ctx.output_handler.clone(),
            ctx.output_stream,
            false,
            ctx.git_revert_mode,
            ctx.revert_prompt.clone(),
            "Code review",
            PhaseType::Review,
            phase_session_id,
            ctx.execution_timings,
            ctx.task_id,
            ctx.plugins,
        )?;

        if ctx.is_final_iteration {
            finalization::complete_final_review(ctx, output.session_id)
        } else {
            non_final::complete_non_final_review(ctx, output.session_id)
        }
    })
}

fn build_continue_session(
    ctx: &PhaseInvocation<'_>,
    session_id: Option<String>,
) -> supervision::ContinueSession {
    supervision::ContinueSession {
        runner: ctx.settings.runner.clone(),
        model: ctx.settings.model.clone(),
        reasoning_effort: ctx.settings.reasoning_effort,
        runner_cli: ctx.settings.runner_cli,
        phase_type: PhaseType::Review,
        session_id,
        output_handler: ctx.output_handler.clone(),
        output_stream: ctx.output_stream,
        ci_failure_retry_count: 0,
        task_id: ctx.task_id.to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    }
}

fn record_phase3_duration(ctx: &PhaseInvocation<'_>, elapsed: Duration) {
    if let Some(timings) = ctx.execution_timings {
        timings.borrow_mut().record_runner_duration(
            PhaseType::Review,
            &ctx.settings.runner,
            &ctx.settings.model,
            elapsed,
        );
    }
}
