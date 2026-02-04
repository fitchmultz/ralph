//! Shared queue CLI enums and conversions.
//!
//! Responsibilities:
//! - Define shared clap enums used by queue/task commands.
//! - Provide lightweight conversions for report/status types.
//!
//! Not handled here:
//! - Command handlers or IO.
//! - Business logic for queue mutations or reporting.
//!
//! Invariants/assumptions:
//! - Enum variants map 1:1 with CLI strings.
//! - Conversions are lossless and do not validate data.

use clap::ValueEnum;

use crate::{contracts, reports};

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum StatusArg {
    /// Task is a draft and not ready to run.
    Draft,
    /// Task is waiting to be started.
    Todo,
    /// Task is currently being worked on.
    Doing,
    /// Task is complete.
    Done,
    /// Task was rejected (dependents can proceed).
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueShowFormat {
    /// Full JSON representation of the task.
    Json,
    /// Compact tab-separated summary (ID, status, title).
    Compact,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueListFormat {
    /// Compact tab-separated summary (ID, status, title).
    Compact,
    /// Detailed tab-separated format including tags, scope, and timestamps.
    Long,
    /// JSON array of task objects (same shape as queue export).
    Json,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueReportFormat {
    /// Human-readable report output.
    Text,
    /// JSON output for scripting.
    Json,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueExportFormat {
    /// Comma-separated values (CSV) format.
    Csv,
    /// Tab-separated values (TSV) format.
    Tsv,
    /// JSON format (array of task objects).
    Json,
    /// Markdown table format for human-readable output.
    Md,
    /// GitHub issue format optimized for issue bodies.
    Gh,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueSortBy {
    /// Sort by priority.
    Priority,
}

impl std::fmt::Display for QueueSortBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueSortBy::Priority => f.write_str("priority"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum QueueSortOrder {
    Ascending,
    Descending,
}

impl QueueSortOrder {
    pub(crate) fn is_descending(self) -> bool {
        matches!(self, QueueSortOrder::Descending)
    }
}

impl std::fmt::Display for QueueSortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueSortOrder::Ascending => f.write_str("ascending"),
            QueueSortOrder::Descending => f.write_str("descending"),
        }
    }
}

impl From<StatusArg> for contracts::TaskStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Draft => contracts::TaskStatus::Draft,
            StatusArg::Todo => contracts::TaskStatus::Todo,
            StatusArg::Doing => contracts::TaskStatus::Doing,
            StatusArg::Done => contracts::TaskStatus::Done,
            StatusArg::Rejected => contracts::TaskStatus::Rejected,
        }
    }
}

impl From<QueueReportFormat> for reports::ReportFormat {
    fn from(value: QueueReportFormat) -> Self {
        match value {
            QueueReportFormat::Text => reports::ReportFormat::Text,
            QueueReportFormat::Json => reports::ReportFormat::Json,
        }
    }
}
