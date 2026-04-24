//! Threshold and anchor-aging tests.
//!
//! Purpose:
//! - Threshold and anchor-aging tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;
use crate::reports::aging::compute::{AgingBucket, anchor_for_task, compute_task_aging};
use crate::reports::aging::thresholds::{
    AgingThresholds, DEFAULT_ROTTEN_DAYS, DEFAULT_STALE_DAYS, DEFAULT_WARNING_DAYS,
};

#[test]
fn anchor_for_task_todo_uses_created_at() {
    let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
    task.created_at = Some("2024-01-01T00:00:00Z".to_string());
    assert_eq!(
        anchor_for_task(&task),
        Some(("created_at", "2024-01-01T00:00:00Z"))
    );
}

#[test]
fn anchor_for_task_doing_prefers_started_at() {
    let mut task = task_with_status("RQ-0001", TaskStatus::Doing);
    task.created_at = Some("2024-01-01T00:00:00Z".to_string());
    task.started_at = Some("2024-01-01T12:00:00Z".to_string());
    assert_eq!(
        anchor_for_task(&task),
        Some(("started_at", "2024-01-01T12:00:00Z"))
    );
}

#[test]
fn anchor_for_task_done_prefers_completed_at() {
    let mut task = task_with_status("RQ-0001", TaskStatus::Done);
    task.created_at = Some("2024-01-01T00:00:00Z".to_string());
    task.updated_at = Some("2024-01-02T00:00:00Z".to_string());
    task.completed_at = Some("2024-01-03T00:00:00Z".to_string());
    assert_eq!(
        anchor_for_task(&task),
        Some(("completed_at", "2024-01-03T00:00:00Z"))
    );
}

#[test]
fn compute_task_aging_assigns_expected_buckets() {
    let thresholds = AgingThresholds::default();
    let now = fixed_now();

    let mut fresh = task_with_status("RQ-FRESH", TaskStatus::Todo);
    fresh.created_at = Some("2023-12-27T00:00:00Z".to_string());
    assert_eq!(
        compute_task_aging(&fresh, thresholds, now).bucket,
        AgingBucket::Fresh
    );

    let mut warning = task_with_status("RQ-WARN", TaskStatus::Todo);
    warning.created_at = Some("2023-12-25T00:00:00Z".to_string());
    assert_eq!(
        compute_task_aging(&warning, thresholds, now).bucket,
        AgingBucket::Warning
    );

    let mut stale = task_with_status("RQ-STALE", TaskStatus::Todo);
    stale.created_at = Some("2023-12-18T00:00:00Z".to_string());
    assert_eq!(
        compute_task_aging(&stale, thresholds, now).bucket,
        AgingBucket::Stale
    );

    let mut rotten = task_with_status("RQ-ROTTEN", TaskStatus::Todo);
    rotten.created_at = Some("2023-12-02T00:00:00Z".to_string());
    assert_eq!(
        compute_task_aging(&rotten, thresholds, now).bucket,
        AgingBucket::Rotten
    );
}

#[test]
fn compute_task_aging_handles_boundaries_and_invalid_values() {
    let thresholds = AgingThresholds::default();
    let now = fixed_now();

    let mut boundary = task_with_status("RQ-BOUNDARY", TaskStatus::Todo);
    boundary.created_at = Some(timeutil::format_rfc3339(now - Duration::days(7)).unwrap());
    assert_eq!(
        compute_task_aging(&boundary, thresholds, now).bucket,
        AgingBucket::Fresh
    );

    let mut future = task_with_status("RQ-FUTURE", TaskStatus::Todo);
    future.created_at = Some("2025-01-01T00:00:00Z".to_string());
    assert_eq!(
        compute_task_aging(&future, thresholds, now).bucket,
        AgingBucket::Unknown
    );

    let missing = task_with_status("RQ-MISSING", TaskStatus::Todo);
    assert_eq!(
        compute_task_aging(&missing, thresholds, now).bucket,
        AgingBucket::Unknown
    );
}

#[test]
fn thresholds_from_queue_config_uses_defaults_and_validates_order() {
    let defaults = AgingThresholds::from_queue_config(&QueueConfig::default()).unwrap();
    assert_eq!(defaults.warning_days, DEFAULT_WARNING_DAYS);
    assert_eq!(defaults.stale_days, DEFAULT_STALE_DAYS);
    assert_eq!(defaults.rotten_days, DEFAULT_ROTTEN_DAYS);

    let valid = QueueConfig {
        aging_thresholds: Some(crate::contracts::QueueAgingThresholds {
            warning_days: Some(5),
            stale_days: Some(10),
            rotten_days: Some(20),
        }),
        ..Default::default()
    };
    let valid = AgingThresholds::from_queue_config(&valid).unwrap();
    assert_eq!(valid.warning_days, 5);

    let invalid = QueueConfig {
        aging_thresholds: Some(crate::contracts::QueueAgingThresholds {
            warning_days: Some(30),
            stale_days: Some(14),
            rotten_days: Some(7),
        }),
        ..Default::default()
    };
    assert!(AgingThresholds::from_queue_config(&invalid).is_err());
}
