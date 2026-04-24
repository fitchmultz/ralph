//! Runnability analysis facade for queue tasks.
//!
//! Purpose:
//! - Runnability analysis facade for queue tasks.
//!
//! Responsibilities:
//! - Re-export the public report/types API for runnability inspection.
//! - Keep the crate-facing surface stable while delegating focused responsibilities.
//! - Centralize module-level documentation for runnability behavior.
//!
//! Non-scope:
//! - Queue persistence or locking.
//! - Task execution or mutation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Report generation is deterministic for a provided `now` timestamp.
//! - Not-runnable reasons remain ordered: status/flags → dependencies → schedule.

mod analysis;
mod model;
mod report;

pub use model::{
    DependencyIssue, NotRunnableReason, QueueRunnabilityReport, QueueRunnabilitySelection,
    QueueRunnabilitySummary, RUNNABILITY_REPORT_VERSION, TaskRunnabilityRow,
};
pub use report::{
    is_task_runnable_detailed, queue_runnability_report, queue_runnability_report_at,
};
