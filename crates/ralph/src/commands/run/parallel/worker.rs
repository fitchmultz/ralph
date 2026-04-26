//! Worker lifecycle facade for parallel task execution.
//!
//! Purpose:
//! - Worker lifecycle facade for parallel task execution.
//!
//! Responsibilities:
//! - Re-export task selection, worker command construction, and process helpers.
//! - Keep implementation modules focused while preserving the existing worker API.
//!
//! Non-scope:
//! - Parallel orchestration loop control.
//! - Persistent worker state serialization.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

#[path = "worker_command.rs"]
mod command;
#[path = "worker_process.rs"]
mod process;
#[path = "worker_selection.rs"]
mod selection;

#[cfg(test)]
pub(crate) use command::{build_worker_command, debug_command_args};
pub(crate) use process::{
    FinishedWorker, WorkerState, spawn_worker, start_worker_monitor, terminate_workers,
};
pub(crate) use selection::{
    NextTaskSelection, collect_excluded_ids, select_next_task_locked, select_next_task_state_locked,
};

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
