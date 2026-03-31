//! `ralph machine` CLI facade.
//!
//! Purpose:
//! - Provide the stable machine-command entrypoint for `main.rs`, tests, and the macOS app.
//!
//! Responsibilities:
//! - Re-export the machine-facing Clap surface consumed by the macOS app.
//! - Keep routing and JSON/document helpers in focused companion modules.
//! - Re-export queue/task machine documents from their companion builders.
//!
//! Scope:
//! - Facade and re-exports only.
//! - Does not own queue/task/run business logic.
//! - Does not define machine contract types.
//!
//! Usage:
//! - Import `crate::cli::machine::*` or call `handle_machine` from CLI entrypoints.
//! - The queue continuation document builders live in `queue_docs.rs` and are re-exported here.
//!
//! Invariants/assumptions:
//! - Machine responses remain versioned and deterministic.
//! - This facade stays thin as machine sub-surfaces evolve.

mod args;
mod common;
mod handle;
mod io;
mod queue;
mod queue_docs;
mod run;
mod task;

pub use args::{
    MachineArgs, MachineCommand, MachineConfigArgs, MachineConfigCommand, MachineDashboardArgs,
    MachineDoctorArgs, MachineDoctorCommand, MachineQueueArgs, MachineQueueCommand,
    MachineQueueRepairArgs, MachineQueueUndoArgs, MachineRunArgs, MachineRunCommand,
    MachineRunLoopArgs, MachineRunOneArgs, MachineSystemArgs, MachineSystemCommand,
    MachineTaskArgs, MachineTaskCommand, MachineTaskCreateArgs, MachineTaskDecomposeArgs,
    MachineTaskMutateArgs,
};
pub use handle::handle_machine;
pub(crate) use queue_docs::{
    build_repair_document as build_queue_repair_document,
    build_undo_document as build_queue_undo_document,
    build_validate_document as build_queue_validate_document,
};
pub(crate) use task::{
    build_decompose_document as build_task_decompose_document, build_task_mutation_document,
};
