//! Purpose: Facade for done-queue pruning surfaces.
//!
//! Responsibilities:
//! - Declare focused prune companion modules.
//! - Re-export the stable prune API used by queue callers.
//! - Keep prune regression coverage colocated with the prune module.
//!
//! Scope:
//! - Thin module root only; prune types and behavior live in sibling companions.
//!
//! Usage:
//! - Used through `crate::queue::{PruneOptions, PruneReport, prune_done_tasks}`.
//! - Crate-internal helpers remain available through `crate::queue::prune::*` re-exports.
//!
//! Invariants/Assumptions:
//! - Re-exports preserve existing caller imports.
//! - Core prune behavior remains unchanged while implementation is split across companions.

mod core;
mod types;

#[cfg(test)]
mod tests;

pub use core::prune_done_tasks;
pub use types::{PruneOptions, PruneReport};
