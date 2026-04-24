//! Stats breakdown helpers.
//!
//! Purpose:
//! - Stats breakdown helpers.
//!
//! Responsibilities:
//! - Compute rolling velocity breakdowns by tag and runner.
//! - Compute slow-group medians by tag and runner.
//!
//! Not handled here:
//! - Summary counts or time-tracking aggregates.
//! - Report rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Velocity uses completion timestamps from terminal tasks only.
//! - Slow groups use positive work durations only.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime};

use crate::contracts::{Task, TaskStatus};
use crate::timeutil;

use super::super::shared::format_duration;
use super::model::{SlowGroupEntry, SlowGroups, VelocityBreakdownEntry, VelocityBreakdowns};
use super::summary::task_runner_group_key;

pub(super) fn calc_velocity_breakdowns(tasks: &[&Task]) -> VelocityBreakdowns {
    let now = OffsetDateTime::now_utc();
    let seven_days_ago = now - Duration::days(7);
    let thirty_days_ago = now - Duration::days(30);

    let mut tag_counts_7: HashMap<String, u32> = HashMap::new();
    let mut tag_counts_30: HashMap<String, u32> = HashMap::new();
    let mut runner_counts_7: HashMap<String, u32> = HashMap::new();
    let mut runner_counts_30: HashMap<String, u32> = HashMap::new();

    for task in tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Done || task.status == TaskStatus::Rejected)
    {
        if let Some(completed_at) = &task.completed_at
            && let Ok(completed_at) = timeutil::parse_rfc3339(completed_at)
        {
            for tag in &task.tags {
                let normalized = tag.to_lowercase();
                if completed_at >= seven_days_ago {
                    *tag_counts_7.entry(normalized.clone()).or_insert(0) += 1;
                }
                if completed_at >= thirty_days_ago {
                    *tag_counts_30.entry(normalized).or_insert(0) += 1;
                }
            }

            if let Some(runner_key) = task_runner_group_key(task) {
                if completed_at >= seven_days_ago {
                    *runner_counts_7.entry(runner_key.clone()).or_insert(0) += 1;
                }
                if completed_at >= thirty_days_ago {
                    *runner_counts_30.entry(runner_key).or_insert(0) += 1;
                }
            }
        }
    }

    let mut by_tag: Vec<VelocityBreakdownEntry> = tag_counts_30
        .keys()
        .map(|key| VelocityBreakdownEntry {
            key: key.clone(),
            last_7_days: *tag_counts_7.get(key).unwrap_or(&0),
            last_30_days: *tag_counts_30.get(key).unwrap_or(&0),
        })
        .collect();
    by_tag.sort_by(|left, right| right.last_30_days.cmp(&left.last_30_days));

    let mut by_runner: Vec<VelocityBreakdownEntry> = runner_counts_30
        .keys()
        .map(|key| VelocityBreakdownEntry {
            key: key.clone(),
            last_7_days: *runner_counts_7.get(key).unwrap_or(&0),
            last_30_days: *runner_counts_30.get(key).unwrap_or(&0),
        })
        .collect();
    by_runner.sort_by(|left, right| right.last_30_days.cmp(&left.last_30_days));

    VelocityBreakdowns { by_tag, by_runner }
}

pub(super) fn calc_slow_groups(tasks: &[&Task]) -> SlowGroups {
    let mut by_tag: HashMap<String, Vec<Duration>> = HashMap::new();
    let mut by_runner: HashMap<String, Vec<Duration>> = HashMap::new();

    for task in tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Done || task.status == TaskStatus::Rejected)
    {
        if let (Some(started), Some(completed)) = (&task.started_at, &task.completed_at)
            && let (Ok(started), Ok(completed)) = (
                timeutil::parse_rfc3339(started),
                timeutil::parse_rfc3339(completed),
            )
            && completed > started
        {
            let duration = completed - started;
            for tag in &task.tags {
                by_tag.entry(tag.to_lowercase()).or_default().push(duration);
            }
            if let Some(runner_key) = task_runner_group_key(task) {
                by_runner.entry(runner_key).or_default().push(duration);
            }
        }
    }

    SlowGroups {
        by_tag: build_slow_group_entries(by_tag),
        by_runner: build_slow_group_entries(by_runner),
    }
}

fn build_slow_group_entries(groups: HashMap<String, Vec<Duration>>) -> Vec<SlowGroupEntry> {
    fn median(durations: &[Duration]) -> Duration {
        let mut sorted = durations.to_vec();
        sorted.sort();
        sorted[sorted.len() / 2]
    }

    let mut entries: Vec<SlowGroupEntry> = groups
        .into_iter()
        .filter(|(_, durations)| !durations.is_empty())
        .map(|(key, durations)| {
            let median = median(&durations);
            SlowGroupEntry {
                key,
                count: durations.len(),
                median_seconds: median.whole_seconds(),
                median_human: format_duration(median),
            }
        })
        .collect();
    entries.sort_by(|left, right| right.median_seconds.cmp(&left.median_seconds));
    entries
}
