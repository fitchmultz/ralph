//! Git workspace helpers for parallel task isolation.
//!
//! Purpose:
//! - Git workspace helpers for parallel task isolation.
//!
//! Responsibilities:
//! - Re-export workspace path, lifecycle, and remote helpers behind a thin facade.
//! - Keep crate-facing git workspace APIs stable while delegating focused concerns.
//! - Centralize module-level invariants for workspace management.
//!
//! Not handled here:
//! - Task orchestration, PR workflows, or integration conflict policy.
//! - Generic filesystem helpers outside workspace lifecycle concerns.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Workspace paths are unique per task ID.
//! - Clones must be retargeted to a pushable `origin` remote before use.

mod git_ops;
mod lifecycle;
mod paths;

pub(crate) use git_ops::origin_urls;
pub(crate) use lifecycle::{WorkspaceSpec, create_workspace_at, remove_workspace};
pub(crate) use paths::workspace_root;

#[cfg(test)]
pub(crate) use lifecycle::ensure_workspace_exists;

#[cfg(test)]
mod tests;
