//! Task aging report implementation.
//!
//! Purpose:
//! - Task aging report implementation.
//!
//! Responsibilities:
//! - Compute aging buckets and report payloads for queue tasks.
//! - Keep threshold validation, aging computation, and rendering in focused helpers.
//! - Expose `compute_task_aging`, `AgingThresholds`, and `print_aging` for report consumers.
//!
//! Not handled here:
//! - Output styling beyond CLI text labels.
//! - Queue persistence or mutation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Threshold ordering is strict: warning < stale < rotten.
//! - Age is always computed from the task-status-specific anchor timestamp.

mod compute;
mod entry;
mod model;
mod render;
mod report;
#[cfg(test)]
mod tests;
mod thresholds;

pub(crate) use entry::print_aging;
pub(crate) use thresholds::AgingThresholds;
