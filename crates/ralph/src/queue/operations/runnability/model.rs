//! Data models for queue runnability reporting.
//!
//! Responsibilities:
//! - Define the serialized report, summary, row, and reason shapes.
//! - Keep JSON field names and report versioning stable.
//! - Provide shared domain enums for runnability callers and tests.
//!
//! Does not handle:
//! - Task analysis logic.
//! - Report aggregation or selection.
//!
//! Invariants/assumptions:
//! - Types are serialized in `snake_case` for CLI/JSON consumers.
//! - `RUNNABILITY_REPORT_VERSION` changes only on intentional schema updates.

use crate::contracts::TaskStatus;
use serde::Serialize;

/// Report version for JSON stability.
pub const RUNNABILITY_REPORT_VERSION: u32 = 1;

/// A structured report of queue runnability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueRunnabilityReport {
    pub version: u32,
    pub now: String,
    pub selection: QueueRunnabilitySelection,
    pub summary: QueueRunnabilitySummary,
    pub tasks: Vec<TaskRunnabilityRow>,
}

/// Selection context for the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueRunnabilitySelection {
    pub include_draft: bool,
    pub prefer_doing: bool,
    pub selected_task_id: Option<String>,
    pub selected_task_status: Option<TaskStatus>,
}

/// Summary counts of runnability states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueRunnabilitySummary {
    pub total_active: usize,
    pub candidates_total: usize,
    pub runnable_candidates: usize,
    pub blocked_by_dependencies: usize,
    pub blocked_by_schedule: usize,
    pub blocked_by_status_or_flags: usize,
}

/// Per-task runnability row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskRunnabilityRow {
    pub id: String,
    pub status: TaskStatus,
    pub runnable: bool,
    pub reasons: Vec<NotRunnableReason>,
}

/// Reason a task is not runnable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotRunnableReason {
    /// Status prevents running (Done/Rejected).
    StatusNotRunnable { status: TaskStatus },
    /// Draft excluded because include_draft is false.
    DraftExcluded,
    /// Dependencies are not met.
    UnmetDependencies { dependencies: Vec<DependencyIssue> },
    /// Scheduled start is in the future.
    ScheduledStartInFuture {
        scheduled_start: String,
        now: String,
        seconds_until_runnable: i64,
    },
}

/// Specific dependency issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DependencyIssue {
    /// Dependency task not found.
    Missing { id: String },
    /// Dependency task exists but is not Done/Rejected.
    NotComplete { id: String, status: TaskStatus },
}
