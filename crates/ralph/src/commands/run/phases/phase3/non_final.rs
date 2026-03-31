//! Purpose: Handle non-final Phase 3 review completion.
//!
//! Responsibilities:
//! - Build the continue-session context for non-final reviews.
//! - Run the optional CI gate with review-session continuation.
//! - Keep Phase 3 duration accounting aligned with resumed review work.
//!
//! Scope:
//! - Non-final review flow only.
//! - Final-iteration supervision and completion enforcement stay in `finalization.rs`.
//!
//! Usage:
//! - Called by `phase3/mod.rs` after a successful non-final review pass.
//!
//! Invariants/Assumptions:
//! - Non-final Phase 3 exits after the CI gate path completes.
//! - Resume-duration tracking reuses the canonical Phase 3 timer helper.

use anyhow::Result;

use super::super::PhaseInvocation;
use super::super::shared::run_ci_gate_with_continue;

pub(super) fn complete_non_final_review(
    ctx: &PhaseInvocation<'_>,
    session_id: Option<String>,
) -> Result<()> {
    let continue_session = super::build_continue_session(ctx, session_id);
    run_ci_gate_with_continue(ctx, continue_session, |_output, elapsed| {
        super::record_phase3_duration(ctx, elapsed);
        Ok(())
    })
}
