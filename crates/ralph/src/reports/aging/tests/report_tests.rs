//! Aggregation-focused aging report tests.
//!
//! Purpose:
//! - Aggregation-focused aging report tests.
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
use crate::reports::aging::report::build_aging_report;
use crate::reports::aging::thresholds::AgingThresholds;

#[test]
fn build_aging_report_filters_statuses_and_builds_expected_buckets() {
    let now = fixed_now();
    let thresholds = AgingThresholds::default();

    let mut fresh = task_with_status("RQ-FRESH", TaskStatus::Todo);
    fresh.created_at = Some(timeutil::format_rfc3339(now - Duration::days(1)).unwrap());

    let mut warning = task_with_status("RQ-WARN", TaskStatus::Todo);
    warning.created_at = Some(timeutil::format_rfc3339(now - Duration::days(8)).unwrap());

    let mut stale = task_with_status("RQ-STALE", TaskStatus::Doing);
    stale.started_at = Some(timeutil::format_rfc3339(now - Duration::days(20)).unwrap());

    let unknown = task_with_status("RQ-UNKNOWN", TaskStatus::Todo);

    let mut excluded_rotten = task_with_status("RQ-EXCLUDED", TaskStatus::Done);
    excluded_rotten.completed_at =
        Some(timeutil::format_rfc3339(now - Duration::days(40)).unwrap());

    let queue = QueueFile {
        version: 1,
        tasks: vec![fresh, warning, stale, unknown, excluded_rotten],
    };

    let report = build_aging_report(
        &queue,
        &[TaskStatus::Todo, TaskStatus::Doing],
        thresholds,
        now,
    );

    assert_eq!(
        report.filters.statuses,
        vec!["todo".to_string(), "doing".to_string()]
    );
    assert_eq!(report.totals.total, 4);
    assert_eq!(report.totals.fresh, 1);
    assert_eq!(report.totals.warning, 1);
    assert_eq!(report.totals.stale, 1);
    assert_eq!(report.totals.rotten, 0);
    assert_eq!(report.totals.unknown, 1);

    let bucket_names: Vec<&str> = report
        .buckets
        .iter()
        .map(|bucket| bucket.bucket.as_str())
        .collect();
    assert_eq!(bucket_names, vec!["stale", "warning", "fresh", "unknown"]);
}

#[test]
fn build_aging_report_sorts_rotten_tasks_by_age_descending() {
    let now = fixed_now();
    let thresholds = AgingThresholds::default();

    let mut older = task_with_status("RQ-OLDER", TaskStatus::Todo);
    older.created_at = Some(timeutil::format_rfc3339(now - Duration::days(60)).unwrap());

    let mut newer = task_with_status("RQ-NEWER", TaskStatus::Todo);
    newer.created_at = Some(timeutil::format_rfc3339(now - Duration::days(40)).unwrap());

    let queue = QueueFile {
        version: 1,
        tasks: vec![newer, older],
    };

    let report = build_aging_report(&queue, &[TaskStatus::Todo], thresholds, now);
    let rotten_bucket = report
        .buckets
        .iter()
        .find(|bucket| bucket.bucket == "rotten")
        .expect("rotten bucket exists");

    assert_eq!(rotten_bucket.tasks[0].id, "RQ-OLDER");
    assert_eq!(rotten_bucket.tasks[1].id, "RQ-NEWER");
    assert!(rotten_bucket.tasks[0].age_seconds > rotten_bucket.tasks[1].age_seconds);
}
