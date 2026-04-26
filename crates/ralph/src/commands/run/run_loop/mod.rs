//! Sequential run-loop facade.
//!
//! Purpose:
//! - Sequential run-loop facade.
//!
//! Responsibilities:
//! - Re-export sequential run-loop entrypoints and options.
//! - Keep orchestration, lifecycle bookkeeping, wait handling, and session recovery separated.
//!
//! Not handled here:
//! - Parallel run-loop orchestration.
//! - Per-task execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue lock contention remains non-retriable.
//! - Session recovery and wait-state handling stay delegated to focused modules.

mod lifecycle;
mod orchestration;
mod session;
mod types;
mod wait;

pub use orchestration::run_loop;
pub use types::{RunLoopOptions, RunLoopOutcome};
