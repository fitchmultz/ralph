//! History report implementation.
//!
//! Purpose:
//! - History report implementation.
//!
//! Responsibilities:
//! - Generate timeline of task creation/completion events by day.
//!
//! Not handled here:
//! - Output formatting (see shared.rs).
//! - CLI argument parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue files are validated before reporting.
//! - Timestamps are RFC3339 format.

use anyhow::Result;
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::contracts::{QueueFile, Task};
use crate::timeutil;

use super::shared::{ReportFormat, format_date_key, print_json};

#[derive(Debug, Serialize)]
pub(crate) struct HistoryWindow {
    pub days: i64,
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct HistoryDay {
    pub date: String,
    pub created: Vec<String>,
    pub completed: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HistoryReport {
    pub window: HistoryWindow,
    pub days: Vec<HistoryDay>,
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

fn collect_all_tasks<'a>(queue: &'a QueueFile, done: Option<&'a QueueFile>) -> Vec<&'a Task> {
    let mut all_tasks: Vec<&Task> = queue.tasks.iter().collect();
    if let Some(done_file) = done {
        all_tasks.extend(done_file.tasks.iter().collect::<Vec<&Task>>());
    }
    all_tasks
}

pub(crate) fn build_history_report(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    days: u32,
) -> HistoryReport {
    build_history_report_at(queue, done, days, OffsetDateTime::now_utc())
}

fn build_history_report_at(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    days: u32,
    now: OffsetDateTime,
) -> HistoryReport {
    let all_tasks = collect_all_tasks(queue, done);
    let days_to_show = days.max(1) as i64;
    let start_of_day = start_of_window(now, days_to_show);
    let end_of_day = start_of_day + Duration::days(days_to_show - 1);

    let mut created_by_day: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut completed_by_day: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for task in all_tasks {
        if let Some(created_ts) = &task.created_at
            && let Ok(dt) = timeutil::parse_rfc3339(created_ts)
            && dt >= start_of_day
        {
            let day_key = format_date_key(dt);
            created_by_day
                .entry(day_key)
                .or_default()
                .push(task.id.clone());
        }

        if let Some(completed_ts) = &task.completed_at
            && let Ok(dt) = timeutil::parse_rfc3339(completed_ts)
            && dt >= start_of_day
        {
            let day_key = format_date_key(dt);
            completed_by_day
                .entry(day_key)
                .or_default()
                .push(task.id.clone());
        }
    }

    let mut days = Vec::new();
    for i in 0..days_to_show {
        let day_dt = start_of_day + Duration::days(i);
        let day_key = format_date_key(day_dt);
        let created = created_by_day.get(&day_key).cloned().unwrap_or_default();
        let completed = completed_by_day.get(&day_key).cloned().unwrap_or_default();
        days.push(HistoryDay {
            date: day_key,
            created,
            completed,
        });
    }

    HistoryReport {
        window: HistoryWindow {
            days: days_to_show,
            start_date: format_date_key(start_of_day),
            end_date: format_date_key(end_of_day),
        },
        days,
    }
}

/// Print history of task events by day.
///
/// # Arguments
/// * `queue` - Active queue tasks
/// * `done` - Completed tasks (optional)
/// * `days` - Number of days to show (default: 7)
pub(crate) fn print_history(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    days: u32,
    format: ReportFormat,
) -> Result<()> {
    let report = build_history_report(queue, done, days);

    match format {
        ReportFormat::Json => {
            print_json(&report)?;
        }
        ReportFormat::Text => {
            println!(
                "Task History (last {} day{})",
                report.window.days,
                if report.window.days == 1 { "" } else { "s" }
            );
            println!(
                "================{}",
                "=".repeat(if report.window.days == 1 { 11 } else { 12 })
            );
            println!();

            let mut has_events = false;

            for day in &report.days {
                if day.created.is_empty() && day.completed.is_empty() {
                    continue;
                }

                has_events = true;

                println!("{}", day.date);
                if !day.created.is_empty() {
                    println!("  Created: {}", day.created.join(", "));
                }
                if !day.completed.is_empty() {
                    println!("  Completed: {}", day.completed.join(", "));
                }
                println!();
            }

            if !has_events {
                println!(
                    "No task creation or completion events in the last {} day{}.",
                    report.window.days,
                    if report.window.days == 1 { "" } else { "s" }
                );
            }
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

    fn test_task(
        id: &str,
        status: TaskStatus,
        created_at: Option<String>,
        completed_at: Option<String>,
    ) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {id}"),
            description: None,
            status,
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
    fn test_collect_all_tasks_includes_queue_and_done() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![Task {
                id: "RQ-1".into(),
                ..Default::default()
            }],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![
                Task {
                    id: "RQ-2".into(),
                    ..Default::default()
                },
                Task {
                    id: "RQ-3".into(),
                    ..Default::default()
                },
            ],
        };

        let all = collect_all_tasks(&queue, Some(&done));
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_build_history_report_buckets_created_and_completed_by_day() {
        let now = fixed_now();
        let start = start_of_window(now, 3);
        let day_one = start + Duration::hours(1);
        let day_two = start + Duration::days(1) + Duration::hours(1);
        let day_three = start + Duration::days(2) + Duration::hours(1);

        let t1 = test_task(
            "RQ-001",
            TaskStatus::Done,
            Some(crate::timeutil::format_rfc3339(day_one).unwrap()),
            Some(crate::timeutil::format_rfc3339(day_two).unwrap()),
        );
        let t2 = test_task(
            "RQ-002",
            TaskStatus::Todo,
            Some(crate::timeutil::format_rfc3339(day_three).unwrap()),
            None,
        );
        let t3 = test_task(
            "RQ-003",
            TaskStatus::Done,
            Some(crate::timeutil::format_rfc3339(day_two).unwrap()),
            Some(crate::timeutil::format_rfc3339(day_three).unwrap()),
        );

        let queue = QueueFile {
            version: 1,
            tasks: vec![t1, t2],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![t3],
        };

        let report = build_history_report_at(&queue, Some(&done), 3, now);

        let day_one_key = format_date_key(day_one);
        let day_two_key = format_date_key(day_two);
        let day_three_key = format_date_key(day_three);

        let day_one_report = report
            .days
            .iter()
            .find(|d| d.date == day_one_key)
            .expect("day one present");
        let day_two_report = report
            .days
            .iter()
            .find(|d| d.date == day_two_key)
            .expect("day two present");
        let day_three_report = report
            .days
            .iter()
            .find(|d| d.date == day_three_key)
            .expect("day three present");

        assert!(day_one_report.created.contains(&"RQ-001".to_string()));
        assert!(day_two_report.created.contains(&"RQ-003".to_string()));
        assert!(day_two_report.completed.contains(&"RQ-001".to_string()));
        assert!(day_three_report.created.contains(&"RQ-002".to_string()));
        assert!(day_three_report.completed.contains(&"RQ-003".to_string()));
    }

    #[test]
    fn test_build_history_report_includes_empty_days_in_window() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![],
        };

        let report = build_history_report(&queue, None, 3);

        assert_eq!(report.days.len(), 3);
        for day in &report.days {
            assert!(day.created.is_empty());
            assert!(day.completed.is_empty());
        }
    }

    #[test]
    fn test_build_history_report_excludes_events_before_window() {
        let now = fixed_now();
        let old_timestamp = now - Duration::days(30);
        let old_str = crate::timeutil::format_rfc3339(old_timestamp).unwrap();

        let task = test_task(
            "RQ-OLD",
            TaskStatus::Done,
            Some(old_str.clone()),
            Some(old_str),
        );

        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };

        let report = build_history_report_at(&queue, None, 7, now);

        for day in &report.days {
            assert!(!day.created.contains(&"RQ-OLD".to_string()));
            assert!(!day.completed.contains(&"RQ-OLD".to_string()));
        }
    }

    #[test]
    fn test_build_history_report_excludes_future_events_outside_window() {
        let now = fixed_now();
        let future_timestamp = now + Duration::days(3);
        let future_str = crate::timeutil::format_rfc3339(future_timestamp).unwrap();

        let task = test_task(
            "RQ-FUTURE",
            TaskStatus::Done,
            Some(future_str.clone()),
            Some(future_str),
        );

        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };

        let report = build_history_report_at(&queue, None, 3, now);

        for day in &report.days {
            assert!(!day.created.contains(&"RQ-FUTURE".to_string()));
            assert!(!day.completed.contains(&"RQ-FUTURE".to_string()));
        }
    }
}
