//! Burndown report implementation.
//!
//! Responsibilities:
//! - Generate burndown chart of remaining tasks over time.
//!
//! Not handled here:
//! - Output formatting (see shared.rs).
//! - CLI argument parsing.
//!
//! Invariants/assumptions:
//! - Queue files are validated before reporting.
//! - Timestamps are RFC3339 format.

use anyhow::Result;
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::contracts::QueueFile;
use crate::timeutil;

use super::shared::{ReportFormat, format_date_key, print_json};

#[derive(Debug, Serialize)]
pub(crate) struct BurndownWindow {
    pub days: i64,
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct BurndownDay {
    pub date: String,
    pub remaining: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct BurndownLegend {
    pub scale_per_block: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct BurndownReport {
    pub window: BurndownWindow,
    pub daily_counts: Vec<BurndownDay>,
    pub max_count: usize,
    pub legend: Option<BurndownLegend>,
}

fn start_of_window(now: OffsetDateTime, days_to_show: i64) -> OffsetDateTime {
    // These time component replacements (0 hour, minute, second, nanosecond) are always valid
    // for any valid OffsetDateTime. The expect calls document this invariant.
    (now - Duration::days(days_to_show - 1))
        .replace_hour(0)
        .expect("hour 0 is always valid")
        .replace_minute(0)
        .expect("minute 0 is always valid")
        .replace_second(0)
        .expect("second 0 is always valid")
        .replace_nanosecond(0)
        .expect("nanosecond 0 is always valid")
}

pub(crate) fn build_burndown_report(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    days: u32,
) -> BurndownReport {
    build_burndown_report_at(queue, done, days, OffsetDateTime::now_utc())
}

fn build_burndown_report_at(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    days: u32,
    now: OffsetDateTime,
) -> BurndownReport {
    let days_to_show = days.max(1) as i64;
    let start_of_day = start_of_window(now, days_to_show);
    let end_of_day = start_of_day + Duration::days(days_to_show - 1);

    let mut all_tasks: Vec<&crate::contracts::Task> = queue.tasks.iter().collect();
    if let Some(done_file) = done {
        all_tasks.extend(
            done_file
                .tasks
                .iter()
                .collect::<Vec<&crate::contracts::Task>>(),
        );
    }

    let mut daily_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();

    for i in 0..days_to_show {
        let day_dt = start_of_day + Duration::days(i);
        let day_end = day_dt + Duration::days(1) - Duration::seconds(1);

        let mut remaining = 0;
        for task in &all_tasks {
            let created = task
                .created_at
                .as_ref()
                .and_then(|ts| timeutil::parse_rfc3339(ts).ok());
            let completed = task
                .completed_at
                .as_ref()
                .and_then(|ts| timeutil::parse_rfc3339(ts).ok());

            let is_open = match created {
                Some(created_dt) => {
                    let created_before_or_on = created_dt <= day_end;
                    let not_completed_yet = match completed {
                        Some(completed_dt) => completed_dt > day_end,
                        None => true,
                    };
                    created_before_or_on && not_completed_yet
                }
                None => false,
            };

            if is_open {
                remaining += 1;
            }
        }

        let day_key = format_date_key(day_dt);
        daily_counts.insert(day_key, remaining);
    }

    let max_count = *daily_counts.values().max().unwrap_or(&0);
    let legend = if max_count == 0 {
        None
    } else {
        Some(BurndownLegend {
            scale_per_block: (max_count / 20).max(1),
        })
    };

    let daily_counts = daily_counts
        .into_iter()
        .map(|(date, remaining)| BurndownDay { date, remaining })
        .collect();

    BurndownReport {
        window: BurndownWindow {
            days: days_to_show,
            start_date: format_date_key(start_of_day),
            end_date: format_date_key(end_of_day),
        },
        daily_counts,
        max_count,
        legend,
    }
}

/// Print burndown chart of remaining tasks over time.
///
/// # Arguments
/// * `queue` - Active queue tasks
/// * `done` - Completed tasks (optional)
/// * `days` - Number of days to show (default: 7)
pub(crate) fn print_burndown(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    days: u32,
    format: ReportFormat,
) -> Result<()> {
    let report = build_burndown_report(queue, done, days);

    match format {
        ReportFormat::Json => {
            print_json(&report)?;
        }
        ReportFormat::Text => {
            println!(
                "Task Burndown (last {} day{})",
                report.window.days,
                if report.window.days == 1 { "" } else { "s" }
            );
            println!(
                "================{}",
                "=".repeat(if report.window.days == 1 { 11 } else { 12 })
            );
            println!();

            if report.daily_counts.is_empty() {
                println!("No data to display.");
                return Ok(());
            }

            if report.max_count == 0 {
                println!(
                    "No remaining tasks in the last {} day{}.",
                    report.window.days,
                    if report.window.days == 1 { "" } else { "s" }
                );
                return Ok(());
            }

            println!("Remaining Tasks");
            println!();

            for day in &report.daily_counts {
                let bar_len =
                    (day.remaining as f64 / report.max_count as f64 * 20.0).round() as usize;
                let bar = "█".repeat(bar_len);

                println!("  {} | {} {}", day.date, bar, day.remaining);
            }

            println!();
            let scale_per_block = report
                .legend
                .as_ref()
                .map(|legend| legend.scale_per_block)
                .unwrap_or(1);
            println!(
                "█ = ~{} task{}",
                scale_per_block,
                if scale_per_block == 1 { "" } else { "s" }
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
    use std::collections::HashMap;
    use time::{Duration, OffsetDateTime};

    fn test_task(id: &str, created_at: Option<String>, completed_at: Option<String>) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {id}"),
            description: None,
            status: if completed_at.is_some() {
                TaskStatus::Done
            } else {
                TaskStatus::Todo
            },
            created_at: created_at.clone(),
            completed_at,
            updated_at: created_at,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
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

    fn queue_with(tasks: Vec<Task>) -> QueueFile {
        QueueFile { version: 1, tasks }
    }

    fn fixed_now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1767441600).expect("valid test timestamp")
    }

    #[test]
    fn test_start_of_window_normalizes_to_midnight() {
        let now = OffsetDateTime::from_unix_timestamp(1700000000).unwrap();
        let days_to_show = 3;
        let start = start_of_window(now, days_to_show);

        assert_eq!(start.hour(), 0);
        assert_eq!(start.minute(), 0);
        assert_eq!(start.second(), 0);
        assert_eq!(start.nanosecond(), 0);

        let expected_day = now.date() - time::Duration::days(days_to_show - 1);
        assert_eq!(start.date(), expected_day);
    }

    #[test]
    fn test_build_burndown_report_counts_open_tasks_by_day() {
        let now = fixed_now();
        let start = start_of_window(now, 3);
        let three_days_ago = start - Duration::days(1);
        let day_three_start = start + Duration::days(2);

        let three_days_ago_str = crate::timeutil::format_rfc3339(three_days_ago).unwrap();
        let day_three_start_str = crate::timeutil::format_rfc3339(day_three_start).unwrap();
        let day_two_midday_str =
            crate::timeutil::format_rfc3339(start + Duration::days(1) + Duration::hours(12))
                .unwrap();
        let day_two_evening_str =
            crate::timeutil::format_rfc3339(start + Duration::days(1) + Duration::hours(18))
                .unwrap();

        let t1 = test_task("RQ-001", Some(three_days_ago_str.clone()), None);
        let t2 = test_task(
            "RQ-002",
            Some(three_days_ago_str),
            Some(day_three_start_str),
        );
        let t3 = test_task(
            "RQ-003",
            Some(day_two_midday_str),
            Some(day_two_evening_str),
        );

        let queue = queue_with(vec![t1, t3]);
        let done = queue_with(vec![t2]);

        let report = build_burndown_report_at(&queue, Some(&done), 3, now);

        assert_eq!(report.daily_counts.len(), 3);
        assert_eq!(report.max_count, 2);
        assert_eq!(
            report
                .daily_counts
                .iter()
                .map(|d| d.remaining)
                .collect::<Vec<_>>(),
            vec![2, 2, 1]
        );
    }

    #[test]
    fn test_build_burndown_report_legend_none_when_max_zero() {
        let queue = queue_with(vec![]);
        let done = queue_with(vec![]);

        let report = build_burndown_report(&queue, Some(&done), 2);

        assert_eq!(report.max_count, 0);
        assert!(report.legend.is_none());
        assert_eq!(report.daily_counts.len(), 2);
    }

    #[test]
    fn test_build_burndown_report_legend_scales_for_large_counts() {
        let now = fixed_now();
        let timestamp_str = crate::timeutil::format_rfc3339(now - Duration::days(1)).unwrap();

        let tasks: Vec<Task> = (0..45)
            .map(|i| test_task(&format!("RQ-{:03}", i), Some(timestamp_str.clone()), None))
            .collect();

        let queue = queue_with(tasks);
        let report = build_burndown_report_at(&queue, None, 3, now);

        assert!(report.legend.is_some());
        let legend = report.legend.unwrap();
        assert_eq!(legend.scale_per_block, 2);
    }
}
