//! Session persistence and recovery facade.
//!
//! Purpose:
//! - Session persistence and recovery facade.
//!
//! Responsibilities:
//! - Re-export session persistence, validation, recovery UI, decision modeling, and progress helpers.
//! - Keep the public `crate::session::*` surface stable while implementation stays split.
//!
//! Not handled here:
//! - Queue/run-loop orchestration.
//! - Session state schema definitions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Persistence, validation, recovery prompts, decision shaping, and progress mutation remain separate.
//! - Re-exports preserve existing caller paths.

mod decision;
mod persistence;
mod progress;
mod recovery;
#[cfg(test)]
mod tests;
mod validation;

pub use decision::{
    ResumeBehavior, ResumeDecision, ResumeDecisionMode, ResumeReason, ResumeResolution,
    ResumeScope, ResumeStatus, RunSessionDecisionOptions, resolve_run_session_decision,
};
pub use persistence::{
    clear_session, get_git_head_commit, load_session, save_session, session_exists, session_path,
};
pub use progress::increment_session_progress;
pub use recovery::{prompt_session_recovery, prompt_session_recovery_timeout};
pub use validation::{
    SessionValidationResult, check_session, validate_session, validate_session_with_now,
};
