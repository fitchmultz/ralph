//! Aggregated dashboard report for analytics UI.
//!
//! Purpose:
//! - Aggregated dashboard report for analytics UI.
//!
//! Responsibilities:
//! - Combine all analytics sections into a single response for GUI clients.
//! - Provide per-section status envelopes for partial failure handling.
//!
//! Not handled here:
//! - CLI argument parsing (see cli/queue/dashboard.rs).
//! - Individual section computation (delegates to reports/stats, reports/burndown, etc.).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue files are validated before reporting.
//! - Productivity stats are optional (may not exist for new projects).

use serde::Serialize;

use crate::contracts::QueueFile;
use crate::productivity::{self, ProductivityStats};
use crate::timeutil;

use super::burndown::BurndownReport;
use super::history::HistoryReport;
use super::shared::print_json;
use super::stats::StatsReport;

/// Status of an individual dashboard section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionStatus {
    /// Section computed successfully.
    Ok,
    /// Section data is not available (e.g., no productivity stats).
    Unavailable,
}

/// Wrapper for a section that may have succeeded or failed.
#[derive(Debug, Clone, Serialize)]
pub struct SectionResult<T> {
    /// Status of this section.
    pub status: SectionStatus,
    /// Section data (only present when status is Ok).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Error message (only present when status is Error or Unavailable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl<T> SectionResult<T> {
    /// Create a successful section result.
    pub fn ok(data: T) -> Self {
        Self {
            status: SectionStatus::Ok,
            data: Some(data),
            error_message: None,
        }
    }

    /// Create an unavailable section result.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: SectionStatus::Unavailable,
            data: None,
            error_message: Some(message.into()),
        }
    }
}

/// All dashboard sections with status wrappers.
#[derive(Debug, Serialize)]
pub struct DashboardSections {
    /// Productivity summary (streaks, milestones, recent completions).
    pub productivity_summary: SectionResult<productivity::ProductivitySummaryReport>,
    /// Productivity velocity (tasks per day over window).
    pub productivity_velocity: SectionResult<productivity::ProductivityVelocityReport>,
    /// Burndown chart data.
    pub burndown: SectionResult<BurndownReport>,
    /// Queue statistics.
    pub queue_stats: SectionResult<StatsReport>,
    /// Task history timeline.
    pub history: SectionResult<HistoryReport>,
}

/// Aggregated dashboard response.
#[derive(Debug, Serialize)]
pub struct DashboardReport {
    /// Time window in days for time-based analytics.
    pub window_days: u32,
    /// ISO8601 timestamp when this report was generated.
    pub generated_at: String,
    /// All dashboard sections with status wrappers.
    pub sections: DashboardSections,
}

/// Build the aggregated dashboard report.
///
/// # Arguments
/// * `queue` - Active queue tasks
/// * `done` - Completed tasks archive (optional)
/// * `stats` - Productivity stats (optional, may not exist for new projects)
/// * `days` - Time window in days for velocity, burndown, history
pub fn build_dashboard_report(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    stats: Option<&ProductivityStats>,
    days: u32,
) -> DashboardReport {
    let generated_at = timeutil::now_utc_rfc3339_or_fallback();

    // Productivity summary
    let productivity_summary = match stats {
        Some(s) => {
            let report = productivity::build_summary_report(s, 5);
            SectionResult::ok(report)
        }
        None => SectionResult::unavailable("productivity stats not available"),
    };

    // Productivity velocity
    let productivity_velocity = match stats {
        Some(s) => {
            let report = productivity::build_velocity_report(s, days);
            SectionResult::ok(report)
        }
        None => SectionResult::unavailable("productivity stats not available"),
    };

    // Burndown
    let burndown_report = super::burndown::build_burndown_report(queue, done, days);
    let burndown = SectionResult::ok(burndown_report);

    // Queue stats
    let stats_report = super::stats::build_stats_report(queue, done, &[]);
    let queue_stats = SectionResult::ok(stats_report);

    // History
    let history_report = super::history::build_history_report(queue, done, days);
    let history = SectionResult::ok(history_report);

    DashboardReport {
        window_days: days,
        generated_at,
        sections: DashboardSections {
            productivity_summary,
            productivity_velocity,
            burndown,
            queue_stats,
            history,
        },
    }
}

/// Print the aggregated dashboard report as JSON.
///
/// The dashboard command only supports JSON output since it's designed for
/// GUI clients that parse structured data.
pub(crate) fn print_dashboard(report: &DashboardReport) -> anyhow::Result<()> {
    print_json(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskPriority, TaskStatus};
    use std::collections::HashMap;

    fn empty_queue() -> QueueFile {
        QueueFile {
            version: 1,
            tasks: vec![],
        }
    }

    fn task_with_status(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            estimated_minutes: None,
            actual_minutes: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn test_dashboard_report_serializes() {
        let queue = empty_queue();
        let report = build_dashboard_report(&queue, None, None, 30);

        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"window_days\": 30"));
        assert!(json.contains("\"productivity_summary\""));
        assert!(json.contains("\"status\": \"unavailable\""));
    }

    #[test]
    fn test_dashboard_report_with_productivity_stats() {
        let stats = ProductivityStats::default();
        let queue = empty_queue();
        let report = build_dashboard_report(&queue, None, Some(&stats), 7);

        assert_eq!(report.window_days, 7);
        assert_eq!(
            report.sections.productivity_summary.status,
            SectionStatus::Ok
        );
        assert!(report.sections.productivity_summary.data.is_some());
    }

    #[test]
    fn test_dashboard_report_with_tasks() {
        let mut queue = empty_queue();
        queue
            .tasks
            .push(task_with_status("RQ-0001", TaskStatus::Todo));
        queue
            .tasks
            .push(task_with_status("RQ-0002", TaskStatus::Done));

        let report = build_dashboard_report(&queue, None, None, 7);

        // Queue stats should have the tasks
        assert_eq!(report.sections.queue_stats.status, SectionStatus::Ok);
        let stats_data = report.sections.queue_stats.data.as_ref().unwrap();
        assert_eq!(stats_data.summary.total, 2);
    }

    #[test]
    fn test_section_result_ok() {
        let result: SectionResult<String> = SectionResult::ok("test".to_string());
        assert_eq!(result.status, SectionStatus::Ok);
        assert_eq!(result.data, Some("test".to_string()));
        assert!(result.error_message.is_none());
    }

    #[test]
    fn test_section_result_unavailable() {
        let result: SectionResult<String> = SectionResult::unavailable("not found");
        assert_eq!(result.status, SectionStatus::Unavailable);
        assert!(result.data.is_none());
        assert_eq!(result.error_message, Some("not found".to_string()));
    }

    #[test]
    fn test_section_result_serializes_correctly() {
        let result: SectionResult<i32> = SectionResult::ok(42);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"data\":42"));
        assert!(!json.contains("error_message"));
    }

    #[test]
    fn test_dashboard_sections_burndown_history_are_ok_without_productivity_stats() {
        let queue = empty_queue();
        let report = build_dashboard_report(&queue, None, None, 7);

        assert_eq!(
            report.sections.productivity_summary.status,
            SectionStatus::Unavailable
        );
        assert_eq!(
            report.sections.productivity_velocity.status,
            SectionStatus::Unavailable
        );
        assert_eq!(report.sections.burndown.status, SectionStatus::Ok);
        assert_eq!(report.sections.history.status, SectionStatus::Ok);
        assert_eq!(report.sections.queue_stats.status, SectionStatus::Ok);
    }

    #[test]
    fn test_dashboard_includes_done_archive_in_queue_stats() {
        let queue = empty_queue();
        let done = QueueFile {
            version: 1,
            tasks: vec![task_with_status("RQ-0001", TaskStatus::Done)],
        };

        let report = build_dashboard_report(&queue, Some(&done), None, 7);

        assert_eq!(report.sections.queue_stats.status, SectionStatus::Ok);
        let stats_data = report.sections.queue_stats.data.as_ref().unwrap();
        assert_eq!(stats_data.summary.total, 1);
        assert_eq!(stats_data.summary.done, 1);
    }
}
