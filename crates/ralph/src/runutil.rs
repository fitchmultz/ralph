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
//! - Submodules remain cohesive (execution vs revert vs abort vs ci-gate vs retry).
//! - Re-exports preserve the existing public and `pub(crate)` API surface.

mod abort;
mod ci_gate;
mod execution;
mod process_groups;
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

// --- Crate-private API (unchanged call-site paths) ---------------------------

pub(crate) use abort::{
    RunAbort, RunAbortReason, abort_reason, is_dirty_repo_error, is_queue_validation_error,
};
pub(crate) use ci_gate::execute_ci_gate;
pub(crate) use process_groups::isolate_child_process_group;

pub(crate) use execution::{
    RunnerErrorMessages, RunnerExecutionContext, RunnerFailureHandling, RunnerInvocation,
    RunnerRetryState, RunnerSettings, run_prompt_with_handling, should_fallback_to_fresh_continue,
};

pub(crate) use retry::{
    FixedBackoffSchedule, RunnerRetryPolicy, SeededRng, compute_backoff, format_duration,
};
pub(crate) use shell::{
    ManagedCommand, TimeoutClass, execute_checked_command, execute_managed_command,
    sleep_with_cancellation,
};

#[cfg(test)]
pub(crate) use execution::{
    RunnerBackend, RunnerBackendResumeSession, RunnerBackendRunPrompt,
    run_prompt_with_handling_backend,
};
