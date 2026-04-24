//! GitHub PR helpers using the `gh` CLI.
//!
//! Purpose:
//! - GitHub PR helpers using the `gh` CLI.
//!
//! Responsibilities:
//! - Expose the crate-facing PR operations and status types.
//! - Keep the public surface stable while delegating execution/parsing to focused submodules.
//! - Colocate PR-specific tests near the implementation.
//!
//! Not handled here:
//! - Task selection or worker execution (see `commands::run::parallel`).
//! - Direct-push parallel integration logic (see `commands::run::parallel::integration`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `gh` is installed and authenticated for command execution paths.
//! - Repo root points to a GitHub-backed repository.

#![allow(dead_code)]

mod gh;
mod ops;
mod parse;
mod types;

pub(crate) use gh::check_gh_available;
pub(crate) use ops::{create_pr, merge_pr, pr_lifecycle_status, pr_merge_status};
pub(crate) use types::{MergeState, PrInfo, PrLifecycle};

#[cfg(test)]
mod tests;
