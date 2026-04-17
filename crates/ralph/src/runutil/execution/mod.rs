//! Purpose: Runner execution facade with consistent error handling.
//!
//! Responsibilities:
//! - Re-export the runner execution types and orchestration entrypoints.
//! - Keep backend wiring, continue-session policy, retry admission, and orchestration split by concern.
//!
//! Scope:
//! - Thin facade only; implementation lives in sibling execution companion modules.
//!
//! Usage:
//! - Imported through `crate::runutil::execution::*` and re-exported by `crate::runutil`.
//!
//! Invariants/Assumptions:
//! - Re-exports preserve the existing `crate::runutil::execution::*` test surface.
//! - `mod orchestration;` resolves to the directory-backed facade under `execution/orchestration/`.

mod backend;
mod continue_session;
mod orchestration;
mod retry_policy;

#[cfg(test)]
pub(crate) use backend::{RunnerBackend, RunnerBackendResumeSession, RunnerBackendRunPrompt};
pub(crate) use backend::{RunnerErrorMessages, RunnerInvocation};
pub(crate) use continue_session::should_fallback_to_fresh_continue;
pub(crate) use orchestration::run_prompt_with_handling;
#[cfg(test)]
pub(crate) use orchestration::run_prompt_with_handling_backend;
