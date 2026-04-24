//! Stats summary and time-tracking helpers.
//!
//! Purpose:
//! - Stats summary and time-tracking helpers.
//!
//! Responsibilities:
//! - Collect and tag-filter the task set used by stats reports.
//! - Compute terminal summary counts and time-tracking aggregates.
//! - Resolve runner grouping keys shared by multiple breakdowns.
//!
//! Not handled here:
//! - Velocity or slow-group breakdown rendering.
//! - Execution-history ETA calculation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Duration metrics only count positive elapsed intervals.

use time::Duration;

use crate::constants::custom_fields::RUNNER_USED;
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::timeutil;

use super::super::shared::{avg_duration, format_duration};
use super::model::{DurationStats, StatsSummary, TimeTrackingStats};

pub(super) fn task_runner_group_key(task: &Task) -> Option<String> {
    task.custom_fields
        .get(RUNNER_USED)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .or_else(|| {
            task.agent
                .as_ref()
                .and_then(|agent| agent.runner.as_ref())
                .map(|runner| runner.id().to_ascii_lowercase())
        })
}

pub(super) fn summarize_tasks(tasks: &[&Task]) -> StatsSummary {
    let total = tasks.len();
    let done = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Done)
        .count();
    let rejected = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Rejected)
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

pub(super) fn collect_all_tasks<'a>(
    queue: &'a QueueFile,
    done: Option<&'a QueueFile>,
) -> Vec<&'a Task> {
    let mut all_tasks: Vec<&Task> = queue.tasks.iter().collect();
    if let Some(done) = done {
        all_tasks.extend(done.tasks.iter());
    }
    all_tasks
}

pub(super) fn filter_tasks_by_tags<'a>(tasks: Vec<&'a Task>, tags: &[String]) -> Vec<&'a Task> {
    if tags.is_empty() {
        return tasks;
    }

    tasks
        .into_iter()
        .filter(|task| {
            let lowered_tags: Vec<String> =
                task.tags.iter().map(|tag| tag.to_lowercase()).collect();
            tags.iter()
                .any(|tag| lowered_tags.contains(&tag.to_lowercase()))
        })
        .collect()
}

pub(super) fn calc_duration_stats(durations: &[Duration]) -> Option<DurationStats> {
    if durations.is_empty() {
        return None;
    }

    let average = avg_duration(durations);
    let mut sorted = durations.to_vec();
    sorted.sort();
    let median = sorted[sorted.len() / 2];

    Some(DurationStats {
        count: durations.len(),
        average_seconds: average.whole_seconds(),
        median_seconds: median.whole_seconds(),
        average_human: format_duration(average),
        median_human: format_duration(median),
    })
}

pub(super) fn build_time_tracking_stats(tasks: &[&Task]) -> TimeTrackingStats {
    let mut lead_times = Vec::new();
    let mut work_times = Vec::new();
    let mut start_lags = Vec::new();

    for task in tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Done || task.status == TaskStatus::Rejected)
    {
        if let (Some(created), Some(completed)) = (&task.created_at, &task.completed_at)
            && let (Ok(created), Ok(completed)) = (
                timeutil::parse_rfc3339(created),
                timeutil::parse_rfc3339(completed),
            )
            && completed > created
        {
            lead_times.push(completed - created);
        }

        if let (Some(started), Some(completed)) = (&task.started_at, &task.completed_at)
            && let (Ok(started), Ok(completed)) = (
                timeutil::parse_rfc3339(started),
                timeutil::parse_rfc3339(completed),
            )
            && completed > started
        {
            work_times.push(completed - started);
        }

        if let (Some(created), Some(started)) = (&task.created_at, &task.started_at)
            && let (Ok(created), Ok(started)) = (
                timeutil::parse_rfc3339(created),
                timeutil::parse_rfc3339(started),
            )
            && started > created
        {
            start_lags.push(started - created);
        }
    }

    let lead_time = calc_duration_stats(&lead_times);
    TimeTrackingStats {
        lead_time: lead_time.clone(),
        work_time: calc_duration_stats(&work_times),
        start_lag: calc_duration_stats(&start_lags),
    }
}
