//! Run command entrypoints and module wiring.
//!
//! Responsibilities:
//! - Define the public `commands::run` API used by CLI and UI clients.
//! - Re-export stable types used across the crate (e.g., `PhaseType`).
//!
//! Not handled here:
//! - Actual run loop implementation (see `run_loop`).
//! - Run-one orchestration (see `run_one`).
//! - Phase execution internals (see `phases`).
//! - Queue lock management (see `queue_lock`).
//! - Dry-run UX (see `dry_run`).
//!
//! Invariants/assumptions:
//! - Public entrypoint signatures must remain stable for CLI and interactive flows.
//! - All orchestration logic lives in submodules; this file is a thin facade.

mod context;
mod dry_run;
mod execution_history_cli;
mod execution_timings;
mod iteration;
mod logging;
mod merge_agent;
pub mod parallel;
mod phases;
mod queue_lock;
mod run_loop;
mod run_one;
mod run_session;
mod selection;
mod supervision;

// Re-export types that are used by other modules via crate::commands::run::* paths.
// These are used by phase modules.
pub(crate) use supervision::{post_run_supervise, post_run_supervise_parallel_worker};

// Re-export PhaseType for use by runner module.
pub(crate) use phases::PhaseType;

pub use crate::agent::AgentOverrides;

// Re-export parallel state types for UI clients.
pub use parallel::state::{
    ParallelPrLifecycle, ParallelPrRecord, ParallelStateFile, load_state, state_file_path,
};

// Re-export run loop types
pub use run_loop::{RunLoopOptions, run_loop};

// Re-export run-one entrypoints
pub use run_one::{
    RunOutcome, run_one, run_one_parallel_worker, run_one_with_id, run_one_with_id_locked,
};

// Re-export dry-run functions
pub use dry_run::{dry_run_loop, dry_run_one};

// Re-export merge-agent types and handler
pub use merge_agent::{MergeAgentError, MergeAgentResult, handle_merge_agent};

#[cfg(test)]
fn resolve_run_agent_settings(
    resolved: &crate::config::Resolved,
    task: &crate::contracts::Task,
    overrides: &AgentOverrides,
) -> anyhow::Result<crate::runner::AgentSettings> {
    crate::runner::resolve_agent_settings(
        overrides.runner.clone(),
        overrides.model.clone(),
        overrides.reasoning_effort,
        &overrides.runner_cli,
        task.agent.as_ref(),
        &resolved.config.agent,
    )
}

#[cfg(test)]
mod tests;
