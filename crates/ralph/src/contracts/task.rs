//! Purpose: Provide the public task-contract API for Ralph queue entries.
//!
//! Responsibilities:
//! - Declare the `contracts::task` child modules.
//! - Re-export the stable public task-contract surface.
//!
//! Scope:
//! - Thin facade only; implementation lives in sibling files under
//!   `contracts/task/`.
//!
//! Usage:
//! - Import `Task`, `TaskAgent`, `TaskPriority`, and `TaskStatus` through
//!   `crate::contracts` or `crate::contracts::task`.
//!
//! Invariants/Assumptions:
//! - Serde and schemars wire-contract behavior remains unchanged across the
//!   split.
//! - Priority ordering remains critical > high > medium > low.

mod priority;
mod serde_helpers;
mod types;

#[cfg(test)]
mod tests;

pub use priority::TaskPriority;
pub use types::{Task, TaskAgent, TaskStatus};
