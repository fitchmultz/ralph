//! Purpose: Directory-backed facade for runner execution orchestration.
//!
//! Responsibilities:
//! - Declare the focused orchestration implementation and regression-test companions.
//! - Re-export the stable runner-execution entrypoints used by `crate::runutil::execution`.
//!
//! Scope:
//! - Thin facade only; orchestration behavior lives in `core.rs`.
//! - Regression coverage for orchestration-specific behavior lives in `tests.rs`.
//!
//! Usage:
//! - Imported by `runutil/execution/mod.rs` to preserve the existing
//!   `crate::runutil::execution::*` surface.
//!
//! Invariants/Assumptions:
//! - Re-exported function signatures remain unchanged.
//! - Companion modules stay private to the orchestration boundary.

mod core;

#[cfg(test)]
mod tests;

pub(crate) use core::run_prompt_with_handling;
#[cfg(test)]
pub(crate) use core::run_prompt_with_handling_backend;
