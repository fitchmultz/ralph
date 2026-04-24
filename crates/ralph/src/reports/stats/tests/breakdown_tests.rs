//! Breakdown-focused stats tests.
//!
//! Purpose:
//! - Breakdown-focused stats tests.
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
use crate::reports::stats::breakdowns::{calc_slow_groups, calc_velocity_breakdowns};
use crate::reports::stats::build_stats_report;

#[test]
fn calc_velocity_breakdowns_groups_by_custom_fields_runner_used() {
    let now = time::OffsetDateTime::now_utc();
    let completed_at = crate::timeutil::format_rfc3339(now).unwrap();

    let mut first = task_with_status("RQ-0001", TaskStatus::Done);
    first.completed_at = Some(completed_at.clone());
    first
        .custom_fields
        .insert(RUNNER_USED.to_string(), "codex".to_string());

    let mut second = task_with_status("RQ-0002", TaskStatus::Rejected);
    second.completed_at = Some(completed_at.clone());
    second
        .custom_fields
        .insert(RUNNER_USED.to_string(), "codex".to_string());

    let mut third = task_with_status("RQ-0003", TaskStatus::Done);
    third.completed_at = Some(completed_at);
    third
        .custom_fields
        .insert(RUNNER_USED.to_string(), "claude".to_string());

    let refs: Vec<&Task> = vec![&first, &second, &third];
    let breakdowns = calc_velocity_breakdowns(&refs);

    assert_eq!(breakdowns.by_runner.len(), 2);
    assert_eq!(breakdowns.by_runner[0].key, "codex");
    assert_eq!(breakdowns.by_runner[0].last_7_days, 2);
    assert_eq!(breakdowns.by_runner[1].key, "claude");
}

#[test]
fn calc_slow_groups_groups_by_custom_fields_runner_used() {
    let end = time::OffsetDateTime::now_utc();
    let start = end - Duration::hours(1);

    let mut task = task_with_status("RQ-0001", TaskStatus::Done);
    task.started_at = Some(crate::timeutil::format_rfc3339(start).unwrap());
    task.completed_at = Some(crate::timeutil::format_rfc3339(end).unwrap());
    task.custom_fields
        .insert(RUNNER_USED.to_string(), "codex".to_string());

    let slow = calc_slow_groups(&[&task]);
    assert_eq!(slow.by_runner.len(), 1);
    assert_eq!(slow.by_runner[0].key, "codex");
    assert_eq!(slow.by_runner[0].median_seconds, 3600);
}

#[test]
fn build_stats_report_respects_tag_filter_and_time_tracking() {
    let now = time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let start = now - Duration::hours(2);
    let created = now - Duration::hours(3);

    let created = crate::timeutil::format_rfc3339(created).unwrap();
    let started = crate::timeutil::format_rfc3339(start).unwrap();
    let completed = crate::timeutil::format_rfc3339(now).unwrap();

    let mut first = task_with_status("RQ-001", TaskStatus::Done);
    first.tags = vec!["A".to_string()];
    first.created_at = Some(created.clone());
    first.started_at = Some(started.clone());
    first.completed_at = Some(completed.clone());

    let mut second = task_with_status("RQ-002", TaskStatus::Done);
    second.tags = vec!["B".to_string()];
    second.created_at = Some(created);
    second.started_at = Some(started);
    second.completed_at = Some(completed);

    let queue = QueueFile {
        version: 1,
        tasks: vec![first, second],
    };

    let report = build_stats_report(&queue, None, &["A".to_string()]);
    assert_eq!(report.summary.total, 1);
    assert!(report.time_tracking.lead_time.is_some());
    assert!(report.time_tracking.work_time.is_some());
    assert!(report.time_tracking.start_lag.is_some());
    assert_eq!(report.filters.tags, vec!["A".to_string()]);
}
