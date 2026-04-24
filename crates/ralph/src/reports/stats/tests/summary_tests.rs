//! Summary-focused stats tests.
//!
//! Purpose:
//! - Summary-focused stats tests.
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
use crate::reports::stats::summary::{
    build_time_tracking_stats, calc_duration_stats, collect_all_tasks, filter_tasks_by_tags,
    summarize_tasks, task_runner_group_key,
};

#[test]
fn task_runner_group_key_prefers_custom_fields_runner_used() {
    let mut task = task_with_status("RQ-0001", TaskStatus::Done);
    task.custom_fields
        .insert(RUNNER_USED.to_string(), "CoDeX ".to_string());
    task.agent = Some(crate::contracts::TaskAgent {
        runner: Some(crate::contracts::Runner::Claude),
        model: None,
        model_effort: crate::contracts::ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    });

    assert_eq!(task_runner_group_key(&task), Some("codex".to_string()));
}

#[test]
fn task_runner_group_key_falls_back_to_agent_runner() {
    let mut task = task_with_status("RQ-0001", TaskStatus::Done);
    task.agent = Some(crate::contracts::TaskAgent {
        runner: Some(crate::contracts::Runner::Claude),
        model: None,
        model_effort: crate::contracts::ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    });

    assert_eq!(task_runner_group_key(&task), Some("claude".to_string()));
}

#[test]
fn summarize_tasks_terminal_counts_rejected() {
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
fn summarize_tasks_empty() {
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

#[test]
fn filter_tasks_by_tags_is_case_insensitive() {
    let mut first = task_with_status("RQ-001", TaskStatus::Done);
    first.tags = vec!["Important".to_string()];
    let mut second = task_with_status("RQ-002", TaskStatus::Done);
    second.tags = vec!["urgent".to_string()];

    let tasks: Vec<&Task> = vec![&first, &second];
    let filtered = filter_tasks_by_tags(tasks.clone(), &["IMPORTANT".to_string()]);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "RQ-001");

    let filtered = filter_tasks_by_tags(tasks, &["urgent".to_string()]);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "RQ-002");
}

#[test]
fn filter_tasks_by_tags_empty_filter_returns_all() {
    let first = task_with_status("RQ-001", TaskStatus::Done);
    let second = task_with_status("RQ-002", TaskStatus::Done);
    let tasks: Vec<&Task> = vec![&first, &second];
    assert_eq!(filter_tasks_by_tags(tasks, &[]).len(), 2);
}

#[test]
fn calc_duration_stats_empty_returns_none() {
    assert!(calc_duration_stats(&[]).is_none());
}

#[test]
fn calc_duration_stats_even_count_uses_upper_middle_median() {
    let durations = vec![
        Duration::hours(1),
        Duration::hours(2),
        Duration::hours(3),
        Duration::hours(4),
    ];
    let stats = calc_duration_stats(&durations).expect("stats expected");
    assert_eq!(stats.count, 4);
    assert_eq!(stats.median_seconds, Duration::hours(3).whole_seconds());
}

#[test]
fn build_time_tracking_stats_collects_positive_intervals() {
    let now = time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let start = now - Duration::hours(2);
    let created = now - Duration::hours(3);

    let mut task = task_with_status("RQ-001", TaskStatus::Done);
    task.created_at = Some(crate::timeutil::format_rfc3339(created).unwrap());
    task.started_at = Some(crate::timeutil::format_rfc3339(start).unwrap());
    task.completed_at = Some(crate::timeutil::format_rfc3339(now).unwrap());

    let stats = build_time_tracking_stats(&[&task]);
    assert!(stats.lead_time.is_some());
    assert!(stats.work_time.is_some());
    assert!(stats.start_lag.is_some());
}

#[test]
fn collect_all_tasks_merges_queue_and_done() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![task_with_status("RQ-001", TaskStatus::Todo)],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![task_with_status("RQ-002", TaskStatus::Done)],
    };

    let tasks = collect_all_tasks(&queue, Some(&done));
    assert_eq!(tasks.len(), 2);
}
