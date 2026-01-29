//! Task statistics and reporting commands.
//!
//! Responsibilities: provide analytics on task velocity, completion rates, and tag distribution.
//! Supports three report types:
//! - `stats`: Summary statistics (completion rate, avg duration, tag breakdown)
//! - `history`: Timeline of creation/completion events by day
//! - `burndown`: Text chart of remaining tasks over time
//!
//! Not handled: queue persistence, CLI argument parsing, or prompt/runner workflows.
//! Invariants/assumptions: queue files are validated before reporting and timestamps are RFC3339.

use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use time::{Duration, OffsetDateTime};

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::timeutil;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReportFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct StatsSummary {
    total: usize,
    done: usize,
    rejected: usize,
    terminal: usize,
    active: usize,
    terminal_rate: f64,
}

#[derive(Debug, Serialize)]
struct StatsFilters {
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DurationStats {
    count: usize,
    average_seconds: i64,
    median_seconds: i64,
    average_human: String,
    median_human: String,
}

#[derive(Debug, Serialize)]
struct TagBreakdown {
    tag: String,
    count: usize,
    percentage: f64,
}

#[derive(Debug, Serialize)]
struct StatsReport {
    summary: StatsSummary,
    durations: Option<DurationStats>,
    tag_breakdown: Vec<TagBreakdown>,
    filters: StatsFilters,
}

#[derive(Debug, Serialize)]
struct HistoryWindow {
    days: i64,
    start_date: String,
    end_date: String,
}

#[derive(Debug, Serialize)]
struct HistoryDay {
    date: String,
    created: Vec<String>,
    completed: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HistoryReport {
    window: HistoryWindow,
    days: Vec<HistoryDay>,
}

#[derive(Debug, Serialize)]
struct BurndownWindow {
    days: i64,
    start_date: String,
    end_date: String,
}

#[derive(Debug, Serialize)]
struct BurndownDay {
    date: String,
    remaining: usize,
}

#[derive(Debug, Serialize)]
struct BurndownLegend {
    scale_per_block: usize,
}

#[derive(Debug, Serialize)]
struct BurndownReport {
    window: BurndownWindow,
    daily_counts: Vec<BurndownDay>,
    max_count: usize,
    legend: Option<BurndownLegend>,
}

fn summarize_tasks(tasks: &[&Task]) -> StatsSummary {
    let total = tasks.len();
    let done = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done)
        .count();
    let rejected = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Rejected)
        .count();
    let terminal = done + rejected;
    let active = total.saturating_sub(terminal);
    let terminal_rate = if total == 0 {
        0.0
    } else {
        (terminal as f64 / total as f64) * 100.0
    };

    StatsSummary {
        total,
        done,
        rejected,
        terminal,
        active,
        terminal_rate,
    }
}

fn collect_all_tasks<'a>(queue: &'a QueueFile, done: Option<&'a QueueFile>) -> Vec<&'a Task> {
    let mut all_tasks: Vec<&Task> = queue.tasks.iter().collect();
    if let Some(done_file) = done {
        all_tasks.extend(done_file.tasks.iter().collect::<Vec<&Task>>());
    }
    all_tasks
}

fn filter_tasks_by_tags<'a>(tasks: Vec<&'a Task>, tags: &[String]) -> Vec<&'a Task> {
    if tags.is_empty() {
        return tasks;
    }

    tasks
        .into_iter()
        .filter(|t| {
            let task_tags_lower: Vec<String> = t.tags.iter().map(|s| s.to_lowercase()).collect();
            tags.iter()
                .any(|tag| task_tags_lower.contains(&tag.to_lowercase()))
        })
        .collect()
}

fn build_stats_report(queue: &QueueFile, done: Option<&QueueFile>, tags: &[String]) -> StatsReport {
    let all_tasks = collect_all_tasks(queue, done);
    let filtered_tasks = filter_tasks_by_tags(all_tasks, tags);

    let summary = summarize_tasks(&filtered_tasks);

    let mut durations: Vec<Duration> = Vec::new();
    for task in filtered_tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done || t.status == TaskStatus::Rejected)
    {
        if let (Some(created), Some(completed)) = (&task.created_at, &task.completed_at) {
            if let (Ok(start), Ok(end)) = (
                timeutil::parse_rfc3339(created),
                timeutil::parse_rfc3339(completed),
            ) {
                if end > start {
                    durations.push(end - start);
                }
            }
        }
    }

    let durations = if durations.is_empty() {
        None
    } else {
        let avg_duration = avg_duration(&durations);
        let mut sorted_durations = durations.clone();
        sorted_durations.sort();
        let median = sorted_durations[sorted_durations.len() / 2];

        Some(DurationStats {
            count: durations.len(),
            average_seconds: avg_duration.whole_seconds(),
            median_seconds: median.whole_seconds(),
            average_human: format_duration(avg_duration),
            median_human: format_duration(median),
        })
    };

    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    for task in &filtered_tasks {
        for tag in &task.tags {
            let normalized = tag.to_lowercase();
            *tag_counts.entry(normalized).or_insert(0) += 1;
        }
    }
    let mut sorted_tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
    sorted_tags.sort_by(|a, b| b.1.cmp(&a.1));

    let total = summary.total as f64;
    let tag_breakdown = sorted_tags
        .into_iter()
        .map(|(tag, count)| TagBreakdown {
            tag,
            count,
            percentage: if total == 0.0 {
                0.0
            } else {
                (count as f64 / total) * 100.0
            },
        })
        .collect();

    StatsReport {
        summary,
        durations,
        tag_breakdown,
        filters: StatsFilters {
            tags: tags.to_vec(),
        },
    }
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

fn build_history_report(queue: &QueueFile, done: Option<&QueueFile>, days: u32) -> HistoryReport {
    let all_tasks = collect_all_tasks(queue, done);
    let days_to_show = days.max(1) as i64;
    let now = OffsetDateTime::now_utc();
    let start_of_day = start_of_window(now, days_to_show);
    let end_of_day = start_of_day + Duration::days(days_to_show - 1);

    let mut created_by_day: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut completed_by_day: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for task in all_tasks {
        if let Some(created_ts) = &task.created_at {
            if let Ok(dt) = timeutil::parse_rfc3339(created_ts) {
                if dt >= start_of_day {
                    let day_key = format_date_key(dt);
                    created_by_day
                        .entry(day_key)
                        .or_default()
                        .push(task.id.clone());
                }
            }
        }

        if let Some(completed_ts) = &task.completed_at {
            if let Ok(dt) = timeutil::parse_rfc3339(completed_ts) {
                if dt >= start_of_day {
                    let day_key = format_date_key(dt);
                    completed_by_day
                        .entry(day_key)
                        .or_default()
                        .push(task.id.clone());
                }
            }
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

fn build_burndown_report(queue: &QueueFile, done: Option<&QueueFile>, days: u32) -> BurndownReport {
    let all_tasks = collect_all_tasks(queue, done);
    let days_to_show = days.max(1) as i64;
    let now = OffsetDateTime::now_utc();
    let start_of_day = start_of_window(now, days_to_show);
    let end_of_day = start_of_day + Duration::days(days_to_show - 1);

    let mut daily_counts: BTreeMap<String, usize> = BTreeMap::new();

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

fn print_json<T: Serialize>(report: &T) -> Result<()> {
    let rendered = serde_json::to_string_pretty(report)?;
    print!("{rendered}");
    Ok(())
}

/// Print summary statistics for tasks.
///
/// # Arguments
/// * `queue` - Active queue tasks
/// * `done` - Completed tasks (optional)
/// * `tags` - Optional tag filter (case-insensitive)
/// * `format` - Output format (text or json)
/// * `queue_file_size_kb` - Size of the queue file in KB for display
pub(crate) fn print_stats(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    tags: &[String],
    format: ReportFormat,
    queue_file_size_kb: u64,
) -> Result<()> {
    let report = build_stats_report(queue, done, tags);

    match format {
        ReportFormat::Json => {
            print_json(&report)?;
        }
        ReportFormat::Text => {
            if report.summary.total == 0 {
                println!("No tasks found.");
                return Ok(());
            }

            println!("Task Statistics");
            println!("================");
            println!();

            println!("Total tasks: {}", report.summary.total);
            println!(
                "Terminal (done/rejected): {} ({:.1}%)",
                report.summary.terminal, report.summary.terminal_rate
            );
            println!("Done: {}", report.summary.done);
            println!("Rejected: {}", report.summary.rejected);
            println!("Active: {}", report.summary.active);
            println!("Queue file size: {}KB", queue_file_size_kb);
            println!();

            if let Some(durations) = &report.durations {
                println!(
                    "Duration Statistics (for {} terminal task{} with valid timestamps):",
                    durations.count,
                    if durations.count == 1 { "" } else { "s" }
                );
                println!("  Average: {}", durations.average_human);
                println!("  Median:  {}", durations.median_human);
                println!();
            }

            if !report.tag_breakdown.is_empty() {
                println!("Tag Breakdown:");
                for entry in &report.tag_breakdown {
                    println!(
                        "  {} ({}: {:.1}%)",
                        entry.count, entry.tag, entry.percentage
                    );
                }
            }
        }
    }

    Ok(())
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
            println!(
                "█ = ~{} task{}",
                (report.max_count / 20).max(1),
                if report.max_count / 20 == 1 { "" } else { "s" }
            );
        }
    }

    Ok(())
}

/// Format a Duration as a human-readable string (e.g., "2h 30m", "1d 4h").
pub(crate) fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.whole_seconds();
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;

    let mut parts = Vec::new();

    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 || days > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 || (hours == 0 && days == 0) {
        parts.push(format!("{}m", minutes));
    }

    if parts.is_empty() {
        "0m".to_string()
    } else {
        parts.join(" ")
    }
}

/// Calculate the average duration from a slice of durations.
fn avg_duration(durations: &[Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }

    let total_seconds: i64 = durations.iter().map(|d| d.whole_seconds()).sum();
    Duration::seconds(total_seconds / durations.len() as i64)
}

/// Format a date as a simple key string (YYYY-MM-DD).
fn format_date_key(dt: OffsetDateTime) -> String {
    format!("{:04}-{:02}-{:02}", dt.year(), dt.month() as u8, dt.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_zero() {
        let duration = Duration::ZERO;
        assert_eq!(format_duration(duration), "0m");
    }

    #[test]
    fn test_format_duration_minutes_only() {
        let duration = Duration::minutes(45);
        assert_eq!(format_duration(duration), "45m");
    }

    #[test]
    fn test_format_duration_hours_and_minutes() {
        let duration = Duration::hours(2) + Duration::minutes(30);
        assert_eq!(format_duration(duration), "2h 30m");
    }

    #[test]
    fn test_format_duration_days_and_hours() {
        let duration = Duration::days(1) + Duration::hours(4) + Duration::minutes(15);
        assert_eq!(format_duration(duration), "1d 4h 15m");
    }

    #[test]
    fn test_format_duration_days_only() {
        let duration = Duration::days(3);
        assert_eq!(format_duration(duration), "3d 0h");
    }

    #[test]
    fn test_avg_duration_empty() {
        let durations: Vec<Duration> = vec![];
        assert_eq!(avg_duration(&durations), Duration::ZERO);
    }

    #[test]
    fn test_avg_duration_single() {
        let durations = vec![Duration::hours(2)];
        assert_eq!(avg_duration(&durations), Duration::hours(2));
    }

    #[test]
    fn test_avg_duration_multiple() {
        let durations = vec![Duration::hours(1), Duration::hours(2), Duration::hours(3)];
        assert_eq!(avg_duration(&durations), Duration::hours(2));
    }

    #[test]
    fn test_format_date_key() {
        let dt = OffsetDateTime::now_utc()
            .replace_year(2026)
            .unwrap()
            .replace_month(time::Month::January)
            .unwrap()
            .replace_day(19)
            .unwrap()
            .replace_hour(12)
            .unwrap()
            .replace_minute(30)
            .unwrap()
            .replace_second(0)
            .unwrap();
        assert_eq!(format_date_key(dt), "2026-01-19");
    }

    fn task_with_status(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_summarize_tasks_terminal_counts_rejected() {
        let tasks = [
            task_with_status("RQ-0001", TaskStatus::Todo),
            task_with_status("RQ-0002", TaskStatus::Doing),
            task_with_status("RQ-0003", TaskStatus::Done),
            task_with_status("RQ-0004", TaskStatus::Rejected),
        ];
        let refs: Vec<&Task> = tasks.iter().collect();
        let summary = summarize_tasks(&refs);

        assert_eq!(summary.total, 4);
        assert_eq!(summary.done, 1);
        assert_eq!(summary.rejected, 1);
        assert_eq!(summary.terminal, 2);
        assert_eq!(summary.active, 2);
        assert!((summary.terminal_rate - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_summarize_tasks_empty() {
        let tasks: Vec<Task> = Vec::new();
        let refs: Vec<&Task> = tasks.iter().collect();
        let summary = summarize_tasks(&refs);

        assert_eq!(summary.total, 0);
        assert_eq!(summary.done, 0);
        assert_eq!(summary.rejected, 0);
        assert_eq!(summary.terminal, 0);
        assert_eq!(summary.active, 0);
        assert_eq!(summary.terminal_rate, 0.0);
    }
}
