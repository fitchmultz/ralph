//! Task decomposition planning and queue materialization helpers.
//!
//! Responsibilities:
//! - Re-export the task decomposition preview/write API from focused companion modules.
//! - Keep the root module as a thin facade for planner, normalization, and queue-write workflows.
//! - Provide a stable import surface for CLI handlers and neighboring task-command modules.
//!
//! Not handled here:
//! - Planner prompt construction or runner invocation details.
//! - Source/attach validation or queue mutation internals.
//! - Tree normalization and materialization helper implementations.
//!
//! Invariants/assumptions:
//! - Preview stays side-effect free with respect to queue/done files.
//! - Write mode re-checks queue state under lock before mutating.

mod planning;
mod resolve;
mod support;
#[cfg(test)]
mod tests;
mod tree;
mod types;
mod write;

pub use planning::plan_task_decomposition;
pub use types::{
    DecompositionAttachTarget, DecompositionChildPolicy, DecompositionPlan, DecompositionPreview,
    DecompositionSource, PlannedNode, TaskDecomposeOptions, TaskDecomposeWriteResult,
};
pub use write::write_task_decomposition;
