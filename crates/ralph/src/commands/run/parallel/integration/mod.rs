//! Worker integration loop for direct-push parallel mode.
//!
//! Purpose:
//! - Worker integration loop for direct-push parallel mode.
//!
//! Responsibilities:
//! - Expose the integration-loop entrypoints used by parallel workers.
//! - Split integration concerns into configuration, persistence, compliance, prompting,
//!   and retry orchestration modules.
//!
//! Not handled here:
//! - Phase execution itself (see `run_one` phase modules).
//! - Worker spawning/orchestration (see `worker.rs` and `orchestration.rs`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Called after the worker has completed its configured phases.
//! - Called only in parallel-worker mode.

mod bookkeeping;
mod compliance;
mod driver;
mod persistence;
mod prompt;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use driver::run_integration_loop;
pub(crate) use persistence::read_blocked_push_marker;
pub use types::{IntegrationConfig, IntegrationOutcome, RemediationHandoff};

#[cfg(test)]
pub(crate) use compliance::{
    ComplianceResult, validate_queue_done_semantics, validate_task_archived,
};
#[cfg(test)]
pub(crate) use prompt::build_agent_integration_prompt;
