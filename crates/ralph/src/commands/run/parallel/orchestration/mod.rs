//! Parallel orchestration facade.
//!
//! Purpose:
//! - Parallel orchestration facade.
//!
//! Responsibilities:
//! - Re-export the direct-push parallel run-loop entrypoint.
//! - Keep preflight, main-loop control, and shutdown/finalization separated.
//!
//! Not handled here:
//! - Parallel state persistence format.
//! - Worker spawn/monitor implementation details.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue lock ownership stays with the orchestration entrypoint for task selection safety.

mod events;
mod loop_runtime;
mod preflight;
mod shutdown;
mod stats;

pub(crate) use loop_runtime::run_loop_parallel;
