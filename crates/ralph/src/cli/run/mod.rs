//! `ralph run ...` facade.
//!
//! Purpose:
//! - `ralph run ...` facade.
//!
//! Responsibilities:
//! - Re-export clap argument types and command handlers for `ralph run`.
//! - Keep clap definitions, long-help content, and dispatch logic in separate modules.
//!
//! Not handled here:
//! - Queue/task execution internals.
//! - Runner implementations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - This module remains a thin facade consistent with other decomposed CLI command groups.

mod args;
mod handle;
mod help;
#[cfg(test)]
mod tests;

pub use args::{
    ParallelArgs, ParallelRetryArgs, ParallelStatusArgs, ParallelSubcommand, ResumeArgs, RunArgs,
    RunCommand, RunLoopArgs, RunOneArgs,
};
pub use handle::handle_run;
