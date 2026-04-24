//! Aging computation helpers.
//!
//! Purpose:
//! - Aging computation helpers.
//!
//! Responsibilities:
//! - Select the correct anchor timestamp for each task status.
//! - Compute age durations and bucket assignments.
//!
//! Not handled here:
//! - Report aggregation or rendering.
//! - Threshold configuration loading.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Future or invalid timestamps produce `Unknown`.

use time::OffsetDateTime;

use crate::contracts::{Task, TaskStatus};
use crate::timeutil;

use super::thresholds::AgingThresholds;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgingBucket {
    Unknown,
    Fresh,
    Warning,
    Stale,
    Rotten,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskAging {
    pub bucket: AgingBucket,
    pub age: Option<time::Duration>,
}

pub(super) fn anchor_for_task(task: &Task) -> Option<(&'static str, &str)> {
    match task.status {
        TaskStatus::Draft | TaskStatus::Todo => task
            .created_at
            .as_deref()
            .map(|timestamp| ("created_at", timestamp)),
        TaskStatus::Doing => task
            .started_at
            .as_deref()
            .map(|timestamp| ("started_at", timestamp))
            .or_else(|| {
                task.created_at
                    .as_deref()
                    .map(|timestamp| ("created_at", timestamp))
            }),
        TaskStatus::Done | TaskStatus::Rejected => task
            .completed_at
            .as_deref()
            .map(|timestamp| ("completed_at", timestamp))
            .or_else(|| {
                task.updated_at
                    .as_deref()
                    .map(|timestamp| ("updated_at", timestamp))
            })
            .or_else(|| {
                task.created_at
                    .as_deref()
                    .map(|timestamp| ("created_at", timestamp))
            }),
    }
}

pub(crate) fn compute_task_aging(
    task: &Task,
    thresholds: AgingThresholds,
    now: OffsetDateTime,
) -> TaskAging {
    let Some((_basis, raw)) = anchor_for_task(task) else {
        return TaskAging {
            bucket: AgingBucket::Unknown,
            age: None,
        };
    };

    let Some(anchor) = timeutil::parse_rfc3339_opt(raw) else {
        return TaskAging {
            bucket: AgingBucket::Unknown,
            age: None,
        };
    };

    if anchor > now {
        return TaskAging {
            bucket: AgingBucket::Unknown,
            age: None,
        };
    }

    let age = now - anchor;
    let bucket = if age > thresholds.rotten_dur() {
        AgingBucket::Rotten
    } else if age > thresholds.stale_dur() {
        AgingBucket::Stale
    } else if age > thresholds.warning_dur() {
        AgingBucket::Warning
    } else {
        AgingBucket::Fresh
    };

    TaskAging {
        bucket,
        age: Some(age),
    }
}
