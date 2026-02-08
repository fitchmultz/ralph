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
pub mod parallel;
mod phases;
mod queue_lock;
mod run_loop;
mod run_one;
mod run_session;
mod selection;
mod supervision;

// Re-export types that are used by other modules within commands::run.
// These are used by the tests module and other internal modules.
#[allow(unused_imports)]
pub(crate) use context::{mark_task_doing, task_context_for_prompt};
#[allow(unused_imports)]
pub(crate) use execution_timings::RunExecutionTimings;
#[allow(unused_imports)]
pub(crate) use iteration::{apply_followup_reasoning_effort, resolve_iteration_settings};
#[allow(unused_imports)]
pub(crate) use run_session::{create_session_for_task, validate_resumed_task};
#[allow(unused_imports)]
pub(crate) use selection::select_run_one_task_index;
#[allow(unused_imports)]
pub(crate) use supervision::{PushPolicy, post_run_supervise, post_run_supervise_parallel_worker};

// Preserve existing `commands::run` unit tests which call phase 3 helpers directly.
#[allow(unused_imports)]
pub(crate) use phases::{apply_phase3_completion_signal, finalize_phase3_if_done};

// Re-export PhaseType for use by runner module.
pub(crate) use phases::PhaseType;

pub use crate::agent::AgentOverrides;

// Re-export parallel state types for UI clients.
pub use parallel::state::{
    ParallelFinishedWithoutPrRecord, ParallelNoPrReason, ParallelPrLifecycle, ParallelPrRecord,
    ParallelStateFile, load_state, state_file_path,
};

// Re-export run loop types
pub use run_loop::{RunLoopOptions, run_loop};

// Re-export run-one entrypoints
pub use run_one::{
    RunOutcome, run_one, run_one_parallel_worker, run_one_with_id, run_one_with_id_locked,
};

// Re-export dry-run functions
pub use dry_run::{dry_run_loop, dry_run_one};

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
