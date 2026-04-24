//! Dependency graph analysis for task queues.
//!
//! Purpose:
//! - Dependency graph analysis for task queues.
//!
//! Responsibilities:
//! - Provide the `queue::graph` module public API via focused submodules.
//! - Re-export the minimal set of types/functions consumed by CLI/GUI clients and other crate code.
//!
//! Not handled here:
//! - Graph construction (see `build`).
//! - Algorithms (see `algorithms`).
//! - Traversal helpers (see `traversal`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The graph represents a DAG in normal operation (cycles are rejected elsewhere).
//! - Task IDs used as graph keys are normalized via `trim()` during graph construction.

mod algorithms;
mod build;
mod traversal;
mod types;

pub use algorithms::{
    find_critical_path_from, find_critical_paths, get_blocked_tasks, get_runnable_tasks,
    topological_sort,
};
pub use build::build_graph;
pub use types::{BoundedChainResult, CriticalPathResult, DependencyGraph, GraphFormat, TaskNode};

#[cfg(test)]
mod tests;
