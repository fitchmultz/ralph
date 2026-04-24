//! Shared queue CLI enums and conversions.
//!
//! Purpose:
//! - Shared queue CLI enums and conversions.
//!
//! Responsibilities:
//! - Define shared clap enums used by queue/task commands.
//! - Provide lightweight conversions for report/status types.
//! - Provide shared ETA computation helpers for queue commands.
//!
//! Not handled here:
//! - Command handlers or IO.
//! - Business logic for queue mutations or reporting.
//! - Actual ETA calculation logic (see `crate::eta_calculator`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Enum variants map 1:1 with CLI strings.
//! - Conversions are lossless and do not validate data.
//! - ETA display uses execution history only (no heuristics).

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
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

/// Import format for `ralph queue import`.
#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueImportFormat {
    /// Comma-separated values (CSV) format.
    Csv,
    /// Tab-separated values (TSV) format.
    Tsv,
    /// JSON format (array of task objects).
    Json,
}

/// Sort-by field for `ralph queue sort` (reorders queue file).
///
/// Intentionally conservative: only supports priority to avoid dangerous
/// "reorder by arbitrary field" footguns that mutate queue.json.
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

/// Sort-by field for `ralph queue list` (sorts output only).
///
/// Supports comprehensive time-based and metadata sorting for triage
/// without the risks of mutating queue.json ordering.
#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueListSortBy {
    /// Sort by priority.
    Priority,
    /// Sort by created_at timestamp.
    CreatedAt,
    /// Sort by updated_at timestamp.
    UpdatedAt,
    /// Sort by started_at timestamp.
    StartedAt,
    /// Sort by scheduled_start timestamp.
    ScheduledStart,
    /// Sort by status lifecycle ordering.
    Status,
    /// Sort by title (case-insensitive).
    Title,
}

impl std::fmt::Display for QueueListSortBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueListSortBy::Priority => f.write_str("priority"),
            QueueListSortBy::CreatedAt => f.write_str("created_at"),
            QueueListSortBy::UpdatedAt => f.write_str("updated_at"),
            QueueListSortBy::StartedAt => f.write_str("started_at"),
            QueueListSortBy::ScheduledStart => f.write_str("scheduled_start"),
            QueueListSortBy::Status => f.write_str("status"),
            QueueListSortBy::Title => f.write_str("title"),
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

/// Compute the ETA display string for a task using execution history.
///
/// Returns "n/a" when:
/// - Task status is not `draft` or `todo` (terminal/in-progress tasks don't need ETA)
/// - No execution history exists for the resolved (runner, model, phase_count) key
/// - Runner/model resolution fails
///
/// Uses `EtaCalculator::estimate_new_task_total` which only returns estimates
/// when actual history samples exist (no heuristic fallbacks).
pub(crate) fn task_eta_display(
    resolved: &crate::config::Resolved,
    calculator: &crate::eta_calculator::EtaCalculator,
    task: &crate::contracts::Task,
) -> String {
    use crate::contracts::TaskStatus;
    use crate::eta_calculator::format_eta;
    use crate::runner::resolve_agent_settings;

    // Only estimate for non-terminal, not-started tasks
    if !matches!(task.status, TaskStatus::Draft | TaskStatus::Todo) {
        return "n/a".to_string();
    }

    // Resolve runner/model using same precedence as runtime (no CLI overrides)
    let empty_cli_patch = crate::contracts::RunnerCliOptionsPatch::default();
    let settings = match resolve_agent_settings(
        None, // runner_override
        None, // model_override
        None, // effort_override
        &empty_cli_patch,
        task.agent.as_ref(),
        &resolved.config.agent,
    ) {
        Ok(s) => s,
        Err(_) => return "n/a".to_string(),
    };

    let phase_count = resolved.config.agent.phases.unwrap_or(3);

    // Get estimate from calculator (returns None if no history)
    match calculator.estimate_new_task_total(
        settings.runner.as_str(),
        settings.model.as_str(),
        phase_count,
    ) {
        Some(estimate) => format_eta(estimate.remaining),
        None => "n/a".to_string(),
    }
}
