//! Run command entrypoints and module wiring.
//!
//! Responsibilities:
//! - Define the public `commands::run` API used by CLI and UI clients.
//! - Re-export stable types used across the crate (e.g., `PhaseType`).
//! - Centralize shared operator-facing run-event types.
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
//! - All orchestration logic lives in submodules; this file is a thin facade plus shared event helpers.

mod context;
mod dry_run;
mod execution_history_cli;
mod execution_timings;
mod iteration;
mod logging;
pub mod parallel;
mod parallel_ops;
mod phases;
mod queue_lock;
mod run_loop;
mod run_one;
mod run_session;
mod selection;
mod supervision;

// Re-export types that are used by other modules via crate::commands::run::* paths.
// These are used by phase modules.
pub(crate) use queue_lock::queue_lock_blocking_state;
pub(crate) use supervision::{CiFailure, post_run_supervise};

// Re-export PhaseType for use by runner module.
pub(crate) use phases::PhaseType;

pub use crate::agent::AgentOverrides;
use crate::contracts::{BlockingReason, BlockingState};
use crate::progress::ExecutionPhase;
use std::sync::Arc;

// Re-export parallel state types for UI clients.
pub use parallel::state::{
    ParallelStateFile, WorkerLifecycle, WorkerRecord, load_state, state_file_path,
};

// Re-export run loop types
pub use run_loop::{RunLoopOptions, run_loop};

// Re-export run-one entrypoints
pub use run_one::{
    RunOneResumeOptions, RunOutcome, run_one, run_one_parallel_worker, run_one_with_handlers,
    run_one_with_id, run_one_with_id_locked,
};

// Re-export dry-run functions
pub use dry_run::{dry_run_loop, dry_run_one};

// Re-export parallel operation commands
pub(crate) use parallel_ops::{build_parallel_status_document, parallel_retry, parallel_status};

#[derive(Debug, Clone)]
pub enum RunEvent {
    ResumeDecision {
        decision: crate::session::ResumeDecision,
    },
    TaskSelected {
        task_id: String,
        title: String,
    },
    PhaseEntered {
        phase: ExecutionPhase,
    },
    PhaseCompleted {
        phase: ExecutionPhase,
    },
    BlockedStateChanged {
        state: BlockingState,
    },
    BlockedStateCleared,
}

pub type RunEventHandler = Arc<Box<dyn Fn(RunEvent) + Send + Sync>>;

pub(crate) fn emit_resume_decision(
    decision: &crate::session::ResumeDecision,
    handler: Option<&RunEventHandler>,
) {
    if let Some(handler) = handler {
        handler(RunEvent::ResumeDecision {
            decision: decision.clone(),
        });
        return;
    }

    eprintln!("{}", decision.message);
    if !decision.detail.trim().is_empty() {
        eprintln!("  {}", decision.detail);
    }
}

pub(crate) fn should_echo_blocked_state_without_handler(state: &BlockingState) -> bool {
    !matches!(state.reason, BlockingReason::RunnerRecovery { .. })
}

pub(crate) fn emit_blocked_state_changed(state: &BlockingState, handler: Option<&RunEventHandler>) {
    if let Some(handler) = handler {
        handler(RunEvent::BlockedStateChanged {
            state: state.clone(),
        });
        return;
    }

    if !should_echo_blocked_state_without_handler(state) {
        return;
    }

    eprintln!("{}", state.message);
    if !state.detail.trim().is_empty() {
        eprintln!("  {}", state.detail);
    }
}

pub(crate) fn emit_blocked_state_cleared(handler: Option<&RunEventHandler>) {
    if let Some(handler) = handler {
        handler(RunEvent::BlockedStateCleared);
    }
}

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
