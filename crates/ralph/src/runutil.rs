//! Shared helpers for runner invocations with consistent error handling.
//!
//! Responsibilities:
//! - Provide a single "runutil" surface for runner execution helpers and revert/abort utilities.
//! - Re-export cohesive submodules so call sites keep using `crate::runutil::{...}`.
//!
//! Not handled here:
//! - Prompt template rendering, queue/task persistence, or runner selection logic.
//!
//! Invariants/assumptions:
//! - Submodules remain cohesive (execution vs revert vs abort vs shell vs retry).
//! - Re-exports preserve the existing public and `pub(crate)` API surface.

mod abort;
mod execution;
mod retry;
mod revert;
mod shell;

#[cfg(test)]
mod tests;

// --- Public API (unchanged call-site paths) ----------------------------------

pub use revert::{
    RevertDecision, RevertOutcome, RevertPromptContext, RevertPromptHandler, RevertSource,
    apply_git_revert_mode, apply_git_revert_mode_with_context, format_revert_failure_message,
    parse_revert_response, prompt_revert_choice_with_io,
};

pub use shell::shell_command;

// --- Crate-private API (unchanged call-site paths) ---------------------------

pub(crate) use abort::{
    RunAbort, RunAbortReason, abort_reason, is_dirty_repo_error, is_queue_validation_error,
};

pub(crate) use execution::{RunnerErrorMessages, RunnerInvocation, run_prompt_with_handling};

pub(crate) use retry::{RunnerRetryPolicy, SeededRng, compute_backoff, format_duration};

#[cfg(test)]
pub(crate) use execution::{RunnerBackend, run_prompt_with_handling_backend};
