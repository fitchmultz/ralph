//! Purpose: Assemble the Phase 3 review prompt.
//!
//! Responsibilities:
//! - Load Phase 3 review/checklist templates.
//! - Render the review body and completion checklist for the current iteration.
//! - Fold in the cached Phase 2 final response when available.
//!
//! Scope:
//! - Prompt/template assembly only.
//! - Runner execution and completion handling stay in sibling modules.
//!
//! Usage:
//! - Called by `phase3/mod.rs` before executing the review runner pass.
//!
//! Invariants/Assumptions:
//! - Missing Phase 2 cache data degrades to a warning plus placeholder text.
//! - Final iterations render the completion checklist; non-final iterations render the iteration checklist.

use anyhow::Result;

use super::super::{PhaseInvocation, PostRunMode};
use crate::{promptflow, prompts};

pub(super) fn build_review_prompt(ctx: &PhaseInvocation<'_>) -> Result<String> {
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
            ctx.post_run_mode == PostRunMode::ParallelWorker,
        )?
    } else {
        let checklist_template = prompts::load_iteration_checklist(&ctx.resolved.repo_root)?;
        prompts::render_iteration_checklist(&checklist_template, ctx.task_id, &ctx.resolved.config)?
    };

    let phase2_final_response =
        match promptflow::read_phase2_final_response_cache(&ctx.resolved.repo_root, ctx.task_id) {
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

    let phase3_template = prompts::load_worker_phase3_prompt(&ctx.resolved.repo_root)?;
    promptflow::build_phase3_prompt(
        &phase3_template,
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
    )
}
