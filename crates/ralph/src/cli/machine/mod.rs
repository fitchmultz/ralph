//! `ralph machine` CLI facade.
//!
//! Responsibilities:
//! - Re-export the machine-facing Clap surface consumed by the macOS app.
//! - Keep routing and JSON/document helpers in focused companion modules.
//! - Preserve the stable public entrypoint for `main.rs` and tests.
//!
//! Not handled here:
//! - Queue/task/run business logic beyond delegated machine handlers.
//! - Machine contract type definitions (see `crate::contracts::machine`).
//! - Human-facing CLI output.
//!
//! Invariants/assumptions:
//! - Machine responses remain versioned and deterministic.
//! - This facade stays thin as machine sub-surfaces evolve.

mod args;
mod common;
mod handle;
mod io;
mod queue;
mod run;
mod task;

pub use args::{
    MachineArgs, MachineCommand, MachineConfigArgs, MachineConfigCommand, MachineDashboardArgs,
    MachineDoctorArgs, MachineDoctorCommand, MachineQueueArgs, MachineQueueCommand, MachineRunArgs,
    MachineRunCommand, MachineRunLoopArgs, MachineRunOneArgs, MachineSystemArgs,
    MachineSystemCommand, MachineTaskArgs, MachineTaskCommand, MachineTaskCreateArgs,
    MachineTaskDecomposeArgs, MachineTaskMutateArgs,
};
pub use handle::handle_machine;
