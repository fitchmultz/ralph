//! Unit tests for runnability analysis.
//!
//! Responsibilities:
//! - Test the runnability report generation and reasoning.
//! - Verify correct identification of blockers (deps, schedule, status).
//!
//! Does not handle:
//! - End-to-end CLI invocation (`ralph queue explain`, `ralph run --dry-run`).
//! - Queue persistence/locking or runner execution.
//!
//! Invariants/assumptions:
//! - Timestamps in tests are RFC3339 and treated as UTC.
//! - Report output is deterministic for the provided `now` timestamp.

use super::{QueueFile, Task, TaskStatus};
use crate::contracts::BlockingReason;
use crate::queue::operations::{
    NotRunnableReason, RunnableSelectionOptions, queue_runnability_report_at,
};
use std::collections::HashMap;

fn make_task_with_deps(
    id: &str,
    status: TaskStatus,
    scheduled: Option<&str>,
    deps: Vec<&str>,
) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: format!("Task {}", id),
        description: None,
        priority: Default::default(),
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: scheduled.map(|s| s.to_string()),
        estimated_minutes: None,
        actual_minutes: None,
        depends_on: deps.into_iter().map(|s| s.to_string()).collect(),
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    }
}

#[test]
fn test_runnability_report_blocked_by_deps() {
    let tasks = vec![
        make_task_with_deps("RQ-0001", TaskStatus::Todo, None, vec!["RQ-0002"]),
        make_task_with_deps("RQ-0002", TaskStatus::Todo, None, vec![]),
    ];
    let active = QueueFile { version: 1, tasks };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        None,
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(!report.tasks[0].runnable);
    assert_eq!(report.summary.blocked_by_dependencies, 1);
    assert!(report.summary.blocking.is_none());
    assert!(matches!(
        report.tasks[0].reasons[0],
        NotRunnableReason::UnmetDependencies { .. }
    ));
}

#[test]
fn test_runnability_report_dependency_only_blocking_state() {
    let tasks = vec![make_task_with_deps(
        "RQ-0001",
        TaskStatus::Todo,
        None,
        vec!["RQ-0002"],
    )];
    let active = QueueFile { version: 1, tasks };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        None,
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(matches!(
        report.summary.blocking.as_ref().map(|state| &state.reason),
        Some(BlockingReason::DependencyBlocked { blocked_tasks: 1 })
    ));
    assert_eq!(
        report
            .summary
            .blocking
            .as_ref()
            .and_then(|s| s.observed_at.as_deref()),
        Some(now),
        "blocking.observed_at should match the runnability report clock"
    );
    assert_eq!(
        report
            .summary
            .blocking
            .as_ref()
            .and_then(|s| s.observed_at.as_deref()),
        Some(report.now.as_str()),
        "blocking.observed_at should match report.now"
    );
}

#[test]
fn test_runnability_report_blocked_by_schedule() {
    let tasks = vec![make_task_with_deps(
        "RQ-0001",
        TaskStatus::Todo,
        Some("2026-12-31T00:00:00Z"),
        vec![],
    )];
    let active = QueueFile { version: 1, tasks };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        None,
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(!report.tasks[0].runnable);
    assert_eq!(report.summary.blocked_by_schedule, 1);
    assert!(matches!(
        report.summary.blocking.as_ref().map(|state| &state.reason),
        Some(BlockingReason::ScheduleBlocked {
            blocked_tasks: 1,
            ..
        })
    ));
    assert!(matches!(
        report.tasks[0].reasons[0],
        NotRunnableReason::ScheduledStartInFuture { .. }
    ));
}

#[test]
fn test_runnability_report_draft_included_with_schedule() {
    let tasks = vec![make_task_with_deps(
        "RQ-0001",
        TaskStatus::Draft,
        Some("2026-12-31T00:00:00Z"),
        vec![],
    )];
    let active = QueueFile { version: 1, tasks };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        None,
        RunnableSelectionOptions::new(true, false),
    )
    .unwrap();

    // Draft included but blocked by schedule
    assert!(!report.tasks[0].runnable);
    assert!(matches!(
        report.tasks[0].reasons[0],
        NotRunnableReason::ScheduledStartInFuture { .. }
    ));
}

#[test]
fn test_runnability_report_draft_excluded() {
    let tasks = vec![make_task_with_deps(
        "RQ-0001",
        TaskStatus::Draft,
        None,
        vec![],
    )];
    let active = QueueFile { version: 1, tasks };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        None,
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(!report.tasks[0].runnable);
    assert!(matches!(
        report.tasks[0].reasons[0],
        NotRunnableReason::DraftExcluded
    ));
}

#[test]
fn test_runnability_report_done_not_runnable() {
    let tasks = vec![make_task_with_deps(
        "RQ-0001",
        TaskStatus::Done,
        None,
        vec![],
    )];
    let active = QueueFile { version: 1, tasks };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        None,
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(!report.tasks[0].runnable);
    assert!(matches!(
        report.tasks[0].reasons[0],
        NotRunnableReason::StatusNotRunnable { status } if status == TaskStatus::Done
    ));
}

#[test]
fn test_runnability_report_prefers_doing_when_prefer_doing_true() {
    let tasks = vec![
        make_task_with_deps("RQ-0001", TaskStatus::Todo, None, vec![]),
        make_task_with_deps("RQ-0002", TaskStatus::Doing, None, vec![]),
    ];
    let active = QueueFile { version: 1, tasks };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        None,
        RunnableSelectionOptions::new(false, true),
    )
    .unwrap();

    assert_eq!(
        report.selection.selected_task_id,
        Some("RQ-0002".to_string())
    );
    assert_eq!(
        report.selection.selected_task_status,
        Some(TaskStatus::Doing)
    );
}

#[test]
fn test_runnability_report_done_dependency_satisfies() {
    let done_tasks = vec![make_task_with_deps(
        "RQ-0002",
        TaskStatus::Done,
        None,
        vec![],
    )];
    let done = QueueFile {
        version: 1,
        tasks: done_tasks,
    };
    let active_tasks = vec![make_task_with_deps(
        "RQ-0001",
        TaskStatus::Todo,
        None,
        vec!["RQ-0002"],
    )];
    let active = QueueFile {
        version: 1,
        tasks: active_tasks,
    };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        Some(&done),
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(report.tasks[0].runnable);
    assert!(report.tasks[0].reasons.is_empty());
}

#[test]
fn test_runnability_report_rejected_dependency_satisfies() {
    let done_tasks = vec![make_task_with_deps(
        "RQ-0002",
        TaskStatus::Rejected,
        None,
        vec![],
    )];
    let done = QueueFile {
        version: 1,
        tasks: done_tasks,
    };
    let active_tasks = vec![make_task_with_deps(
        "RQ-0001",
        TaskStatus::Todo,
        None,
        vec!["RQ-0002"],
    )];
    let active = QueueFile {
        version: 1,
        tasks: active_tasks,
    };
    let now = "2026-01-18T12:00:00Z";

    let report = queue_runnability_report_at(
        now,
        &active,
        Some(&done),
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(report.tasks[0].runnable);
    assert!(report.tasks[0].reasons.is_empty());
}

#[test]
fn test_runnability_report_idle_blocking_state_when_no_candidates_exist() {
    let active = QueueFile {
        version: 1,
        tasks: vec![make_task_with_deps(
            "RQ-0001",
            TaskStatus::Done,
            None,
            vec![],
        )],
    };

    let report = queue_runnability_report_at(
        "2026-01-18T12:00:00Z",
        &active,
        None,
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(matches!(
        report.summary.blocking.as_ref().map(|state| &state.reason),
        Some(BlockingReason::Idle {
            include_draft: false
        })
    ));
}

#[test]
fn test_runnability_report_mixed_queue_blocking_state() {
    let tasks = vec![
        make_task_with_deps("RQ-0001", TaskStatus::Todo, None, vec!["RQ-0002"]),
        make_task_with_deps(
            "RQ-0002",
            TaskStatus::Todo,
            Some("2026-12-31T00:00:00Z"),
            vec![],
        ),
    ];
    let active = QueueFile { version: 1, tasks };

    let report = queue_runnability_report_at(
        "2026-01-18T12:00:00Z",
        &active,
        None,
        RunnableSelectionOptions::new(false, false),
    )
    .unwrap();

    assert!(matches!(
        report.summary.blocking.as_ref().map(|state| &state.reason),
        Some(BlockingReason::MixedQueue {
            dependency_blocked: 1,
            schedule_blocked: 1,
            status_filtered: 0,
        })
    ));
}
