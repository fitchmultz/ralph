//! Task statistics and reporting commands.
//!
//! Provides analytics on task velocity, completion rates, and tag distribution.
//! Supports three report types:
//! - `stats`: Summary statistics (completion rate, avg duration, tag breakdown)
//! - `history`: Timeline of creation/completion events by day
//! - `burndown`: Text chart of remaining tasks over time

use anyhow::Result;
use std::collections::{BTreeMap, HashMap};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::contracts::{QueueFile, Task, TaskStatus};

#[derive(Debug, Clone, Copy, PartialEq)]
struct StatsSummary {
    total: usize,
    done: usize,
    rejected: usize,
    terminal: usize,
    active: usize,
    terminal_rate: f64,
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

/// Print summary statistics for tasks.
///
/// # Arguments
/// * `queue` - Active queue tasks
/// * `done` - Completed tasks (optional)
/// * `tags` - Optional tag filter (case-insensitive)
pub fn print_stats(queue: &QueueFile, done: Option<&QueueFile>, tags: &[String]) -> Result<()> {
    let mut all_tasks: Vec<&Task> = queue.tasks.iter().collect();
    if let Some(done_file) = done {
        all_tasks.extend(done_file.tasks.iter().collect::<Vec<&Task>>());
    }

    // Filter by tags if specified
    let filtered_tasks = if tags.is_empty() {
        all_tasks
    } else {
        all_tasks
            .into_iter()
            .filter(|t| {
                let task_tags_lower: Vec<String> =
                    t.tags.iter().map(|s| s.to_lowercase()).collect();
                tags.iter()
                    .any(|tag| task_tags_lower.contains(&tag.to_lowercase()))
            })
            .collect()
    };

    let total = filtered_tasks.len();
    if total == 0 {
        println!("No tasks found.");
        return Ok(());
    }

    let summary = summarize_tasks(&filtered_tasks);

    // Calculate durations for completed tasks
    let mut durations: Vec<Duration> = Vec::new();
    for task in filtered_tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done || t.status == TaskStatus::Rejected)
    {
        if let (Some(created), Some(completed)) = (&task.created_at, &task.completed_at) {
            if let (Ok(start), Ok(end)) = (parse_ts(created), parse_ts(completed)) {
                if end > start {
                    durations.push(end - start);
                }
            }
        }
    }

    // Tag breakdown
    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    for task in &filtered_tasks {
        for tag in &task.tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }
    let mut sorted_tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
    sorted_tags.sort_by(|a, b| b.1.cmp(&a.1));

    // Print stats
    println!("Task Statistics");
    println!("================");
    println!();

    println!("Total tasks: {}", summary.total);
    println!(
        "Terminal (done/rejected): {} ({:.1}%)",
        summary.terminal, summary.terminal_rate
    );
    println!("Done: {}", summary.done);
    println!("Rejected: {}", summary.rejected);
    println!("Active: {}", summary.active);
    println!();

    if !durations.is_empty() {
        let avg_duration = avg_duration(&durations);
        let mut sorted_durations = durations.clone();
        sorted_durations.sort();
        let median = sorted_durations[sorted_durations.len() / 2];

        println!(
            "Duration Statistics (for {} terminal task{} with valid timestamps):",
            durations.len(),
            if durations.len() == 1 { "" } else { "s" }
        );
        println!("  Average: {}", format_duration(avg_duration));
        println!("  Median:  {}", format_duration(median));
        println!();
    }

    if !sorted_tags.is_empty() {
        println!("Tag Breakdown:");
        for (tag, count) in sorted_tags {
            let percentage = (count as f64 / total as f64) * 100.0;
            println!("  {} ({}: {:.1}%)", count, tag, percentage);
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
pub fn print_history(queue: &QueueFile, done: Option<&QueueFile>, days: u32) -> Result<()> {
    let mut all_tasks: Vec<&Task> = queue.tasks.iter().collect();
    if let Some(done_file) = done {
        all_tasks.extend(done_file.tasks.iter().collect::<Vec<&Task>>());
    }

    let days_to_show = days.max(1) as i64;
    let now = OffsetDateTime::now_utc();
    let start_of_day = (now - Duration::days(days_to_show - 1))
        .replace_hour(0)
        .unwrap()
        .replace_minute(0)
        .unwrap()
        .replace_second(0)
        .unwrap()
        .replace_nanosecond(0)
        .unwrap();

    // Group events by day
    let mut created_by_day: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut completed_by_day: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for task in all_tasks {
        if let Some(created_ts) = &task.created_at {
            if let Ok(dt) = parse_ts(created_ts) {
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
            if let Ok(dt) = parse_ts(completed_ts) {
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

    println!(
        "Task History (last {} day{})",
        days_to_show,
        if days_to_show == 1 { "" } else { "s" }
    );
    println!(
        "================{}",
        "=".repeat(if days_to_show == 1 { 11 } else { 12 })
    );
    println!();

    let mut has_events = false;

    // Iterate through days
    for i in 0..days_to_show {
        let day_dt = start_of_day + Duration::days(i);
        let day_key = format_date_key(day_dt);

        let created = created_by_day
            .get(&day_key)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let completed = completed_by_day
            .get(&day_key)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        if created.is_empty() && completed.is_empty() {
            continue;
        }

        has_events = true;

        println!("{}", day_key);
        if !created.is_empty() {
            println!("  Created: {}", created.join(", "));
        }
        if !completed.is_empty() {
            println!("  Completed: {}", completed.join(", "));
        }
        println!();
    }

    if !has_events {
        println!(
            "No task creation or completion events in the last {} day{}.",
            days_to_show,
            if days_to_show == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Print burndown chart of remaining tasks over time.
///
/// # Arguments
/// * `queue` - Active queue tasks
/// * `done` - Completed tasks (optional)
/// * `days` - Number of days to show (default: 7)
pub fn print_burndown(queue: &QueueFile, done: Option<&QueueFile>, days: u32) -> Result<()> {
    let mut all_tasks: Vec<&Task> = queue.tasks.iter().collect();
    if let Some(done_file) = done {
        all_tasks.extend(done_file.tasks.iter().collect::<Vec<&Task>>());
    }

    let days_to_show = days.max(1) as i64;
    let now = OffsetDateTime::now_utc();
    let start_of_day = (now - Duration::days(days_to_show - 1))
        .replace_hour(0)
        .unwrap()
        .replace_minute(0)
        .unwrap()
        .replace_second(0)
        .unwrap()
        .replace_nanosecond(0)
        .unwrap();

    // Calculate remaining tasks per day
    let mut daily_counts: BTreeMap<String, usize> = BTreeMap::new();

    for i in 0..days_to_show {
        let day_dt = start_of_day + Duration::days(i);
        let day_end = day_dt + Duration::days(1) - Duration::seconds(1);

        let mut remaining = 0;
        for task in &all_tasks {
            let created = task.created_at.as_ref().and_then(|ts| parse_ts(ts).ok());
            let completed = task.completed_at.as_ref().and_then(|ts| parse_ts(ts).ok());

            // Task is "open" if it was created on or before this day AND
            // (not completed OR completed after this day)
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

    println!(
        "Task Burndown (last {} day{})",
        days_to_show,
        if days_to_show == 1 { "" } else { "s" }
    );
    println!(
        "================{}",
        "=".repeat(if days_to_show == 1 { 11 } else { 12 })
    );
    println!();

    if daily_counts.is_empty() {
        println!("No data to display.");
        return Ok(());
    }

    let max_count = *daily_counts.values().max().unwrap_or(&1);

    // Print header
    println!("Remaining Tasks");
    println!();

    // Print each day with a simple bar chart
    for (day_key, count) in &daily_counts {
        let bar_len = (*count as f64 / max_count as f64 * 20.0).round() as usize;
        let bar = "█".repeat(bar_len.max(1));

        println!("  {} | {} {}", day_key, bar, count);
    }

    println!();
    println!(
        "█ = ~{} task{}",
        (max_count / 20).max(1),
        if max_count / 20 == 1 { "" } else { "s" }
    );

    Ok(())
}

/// Parse an RFC3339 timestamp string into OffsetDateTime.
pub fn parse_ts(ts: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(ts, &Rfc3339)
        .map_err(|e| anyhow::anyhow!("Failed to parse timestamp '{}': {}", ts, e))
}

/// Format a Duration as a human-readable string (e.g., "2h 30m", "1d 4h").
pub fn format_duration(duration: Duration) -> String {
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
    fn test_parse_ts_valid() {
        let ts = "2026-01-19T12:00:00Z";
        let result = parse_ts(ts);
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month() as u8, 1);
        assert_eq!(dt.day(), 19);
    }

    #[test]
    fn test_parse_ts_invalid() {
        let ts = "invalid-timestamp";
        let result = parse_ts(ts);
        assert!(result.is_err());
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
