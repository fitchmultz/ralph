//! Aging report aggregation helpers.
//!
//! Purpose:
//! - Aging report aggregation helpers.
//!
//! Responsibilities:
//! - Build the structured aging report from computed bucket assignments.
//! - Sort tasks within buckets and omit low-signal fresh details from text rendering.
//!
//! Not handled here:
//! - Per-task bucket computation.
//! - Text rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Buckets are emitted in severity order: rotten, stale, warning, fresh, unknown.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime};

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::timeutil;

use super::super::shared::format_duration;
use super::compute::{AgingBucket, anchor_for_task, compute_task_aging};
use super::model::{
    AgingBucketEntry, AgingFilters, AgingReport, AgingTaskEntry, AgingThresholdsOutput, AgingTotals,
};
use super::thresholds::AgingThresholds;

pub(super) fn build_aging_report(
    queue: &QueueFile,
    statuses: &[TaskStatus],
    thresholds: AgingThresholds,
    now: OffsetDateTime,
) -> AgingReport {
    let filtered_tasks: Vec<&Task> = queue
        .tasks
        .iter()
        .filter(|task| statuses.contains(&task.status))
        .collect();

    let mut bucketed: HashMap<AgingBucket, Vec<(&Task, Duration)>> = HashMap::new();
    let mut unknown_count = 0usize;

    for task in &filtered_tasks {
        let aging = compute_task_aging(task, thresholds, now);
        if let Some(age) = aging.age {
            bucketed.entry(aging.bucket).or_default().push((task, age));
        } else if aging.bucket == AgingBucket::Unknown {
            unknown_count += 1;
        }
    }

    let fresh_tasks =
        build_bucket_entries(bucketed.remove(&AgingBucket::Fresh).unwrap_or_default());
    let warning_tasks =
        build_bucket_entries(bucketed.remove(&AgingBucket::Warning).unwrap_or_default());
    let stale_tasks =
        build_bucket_entries(bucketed.remove(&AgingBucket::Stale).unwrap_or_default());
    let rotten_tasks =
        build_bucket_entries(bucketed.remove(&AgingBucket::Rotten).unwrap_or_default());

    let totals = AgingTotals {
        total: filtered_tasks.len(),
        fresh: fresh_tasks.len(),
        warning: warning_tasks.len(),
        stale: stale_tasks.len(),
        rotten: rotten_tasks.len(),
        unknown: unknown_count,
    };

    let mut buckets = Vec::new();
    if !rotten_tasks.is_empty() {
        buckets.push(AgingBucketEntry {
            bucket: "rotten".to_string(),
            count: rotten_tasks.len(),
            tasks: rotten_tasks,
        });
    }
    if !stale_tasks.is_empty() {
        buckets.push(AgingBucketEntry {
            bucket: "stale".to_string(),
            count: stale_tasks.len(),
            tasks: stale_tasks,
        });
    }
    if !warning_tasks.is_empty() {
        buckets.push(AgingBucketEntry {
            bucket: "warning".to_string(),
            count: warning_tasks.len(),
            tasks: warning_tasks,
        });
    }
    buckets.push(AgingBucketEntry {
        bucket: "fresh".to_string(),
        count: fresh_tasks.len(),
        tasks: Vec::new(),
    });
    if unknown_count > 0 {
        buckets.push(AgingBucketEntry {
            bucket: "unknown".to_string(),
            count: unknown_count,
            tasks: Vec::new(),
        });
    }

    AgingReport {
        as_of: timeutil::format_rfc3339(now).unwrap_or_else(|_| now.to_string()),
        thresholds: AgingThresholdsOutput {
            warning_days: thresholds.warning_days,
            stale_days: thresholds.stale_days,
            rotten_days: thresholds.rotten_days,
        },
        filters: AgingFilters {
            statuses: statuses
                .iter()
                .map(|status| status.as_str().to_string())
                .collect(),
        },
        totals,
        buckets,
    }
}

fn build_bucket_entries(entries: Vec<(&Task, Duration)>) -> Vec<AgingTaskEntry> {
    let mut entries: Vec<AgingTaskEntry> = entries
        .into_iter()
        .map(|(task, age)| {
            let (basis, anchor_ts) = anchor_for_task(task)
                .map(|(basis, timestamp)| (basis.to_string(), timestamp.to_string()))
                .unwrap_or_else(|| ("unknown".to_string(), String::new()));

            AgingTaskEntry {
                id: task.id.clone(),
                title: task.title.clone(),
                status: task.status,
                age_seconds: age.whole_seconds(),
                age_human: format_duration(age),
                basis,
                anchor_ts,
            }
        })
        .collect();
    entries.sort_by(|left, right| right.age_seconds.cmp(&left.age_seconds));
    entries
}
