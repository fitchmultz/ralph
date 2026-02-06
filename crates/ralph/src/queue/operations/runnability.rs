//! Runnability analysis for queue tasks.
//!
//! Responsibilities:
//! - Determine why tasks are or aren't runnable (status, dependencies, schedule).
//! - Provide structured reports for CLI introspection (`queue explain`, `run --dry-run`).
//!
//! Does not handle:
//! - Task selection or execution (see `crate::queue::operations::query`).
//! - Queue persistence or locking.
//!
//! Invariants/assumptions:
//! - Report generation is deterministic for a given `now` timestamp.
//! - Reasons are ordered: status/flags → dependencies → schedule.

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue::operations::{RunnableSelectionOptions, find_task_across};
use anyhow::Result;
use serde::Serialize;

/// Report version for JSON stability.
pub const RUNNABILITY_REPORT_VERSION: u32 = 1;

/// A structured report of queue runnability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueRunnabilityReport {
    pub version: u32,
    pub now: String,
    pub selection: QueueRunnabilitySelection,
    pub summary: QueueRunnabilitySummary,
    pub tasks: Vec<TaskRunnabilityRow>,
}

/// Selection context for the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueRunnabilitySelection {
    pub include_draft: bool,
    pub prefer_doing: bool,
    pub selected_task_id: Option<String>,
    pub selected_task_status: Option<TaskStatus>,
}

/// Summary counts of runnability states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueRunnabilitySummary {
    pub total_active: usize,
    pub candidates_total: usize,
    pub runnable_candidates: usize,
    pub blocked_by_dependencies: usize,
    pub blocked_by_schedule: usize,
    pub blocked_by_status_or_flags: usize,
}

/// Per-task runnability row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskRunnabilityRow {
    pub id: String,
    pub status: TaskStatus,
    pub runnable: bool,
    pub reasons: Vec<NotRunnableReason>,
}

/// Reason a task is not runnable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotRunnableReason {
    /// Status prevents running (Done/Rejected).
    StatusNotRunnable { status: TaskStatus },
    /// Draft excluded because include_draft is false.
    DraftExcluded,
    /// Dependencies are not met.
    UnmetDependencies { dependencies: Vec<DependencyIssue> },
    /// Scheduled start is in the future.
    ScheduledStartInFuture {
        scheduled_start: String,
        now: String,
        seconds_until_runnable: i64,
    },
}

/// Specific dependency issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DependencyIssue {
    /// Dependency task not found.
    Missing { id: String },
    /// Dependency task exists but is not Done/Rejected.
    NotComplete { id: String, status: TaskStatus },
}

/// Build a runnability report with the current time.
pub fn queue_runnability_report(
    active: &QueueFile,
    done: Option<&QueueFile>,
    options: RunnableSelectionOptions,
) -> Result<QueueRunnabilityReport> {
    let now = crate::timeutil::now_utc_rfc3339()?;
    queue_runnability_report_at(&now, active, done, options)
}

/// Build a runnability report with a specific timestamp (deterministic for tests).
pub fn queue_runnability_report_at(
    now_rfc3339: &str,
    active: &QueueFile,
    done: Option<&QueueFile>,
    options: RunnableSelectionOptions,
) -> Result<QueueRunnabilityReport> {
    let now_dt = crate::timeutil::parse_rfc3339(now_rfc3339)?;

    let mut tasks = Vec::with_capacity(active.tasks.len());
    let mut candidates_total = 0usize;
    let mut runnable_candidates = 0usize;
    let mut blocked_by_deps = 0usize;
    let mut blocked_by_schedule = 0usize;
    let mut blocked_by_status = 0usize;

    // Determine selected task
    let mut selected_task_id: Option<String> = None;
    let mut selected_task_status: Option<TaskStatus> = None;

    for task in &active.tasks {
        let row = analyze_task_runnability(task, active, done, now_rfc3339, now_dt, options);

        // Count candidates (Todo or Draft if include_draft)
        let is_candidate = row.status == TaskStatus::Todo
            || (options.include_draft && row.status == TaskStatus::Draft);

        if is_candidate {
            candidates_total += 1;
            if row.runnable {
                runnable_candidates += 1;
            } else {
                // Count blockers
                for reason in &row.reasons {
                    match reason {
                        NotRunnableReason::StatusNotRunnable { .. }
                        | NotRunnableReason::DraftExcluded => {
                            blocked_by_status += 1;
                        }
                        NotRunnableReason::UnmetDependencies { .. } => {
                            blocked_by_deps += 1;
                        }
                        NotRunnableReason::ScheduledStartInFuture { .. } => {
                            blocked_by_schedule += 1;
                        }
                    }
                }
            }
        }

        tasks.push(row);
    }

    // Find selected task based on options (same logic as select_runnable_task_index)
    if options.prefer_doing
        && let Some(task) = active.tasks.iter().find(|t| t.status == TaskStatus::Doing)
    {
        selected_task_id = Some(task.id.clone());
        selected_task_status = Some(TaskStatus::Doing);
    }

    if selected_task_id.is_none() {
        for row in &tasks {
            if row.runnable {
                if row.status == TaskStatus::Todo {
                    selected_task_id = Some(row.id.clone());
                    selected_task_status = Some(TaskStatus::Todo);
                    break;
                }
                if options.include_draft && row.status == TaskStatus::Draft {
                    selected_task_id = Some(row.id.clone());
                    selected_task_status = Some(TaskStatus::Draft);
                    break;
                }
            }
        }
    }

    Ok(QueueRunnabilityReport {
        version: RUNNABILITY_REPORT_VERSION,
        now: now_rfc3339.to_string(),
        selection: QueueRunnabilitySelection {
            include_draft: options.include_draft,
            prefer_doing: options.prefer_doing,
            selected_task_id,
            selected_task_status,
        },
        summary: QueueRunnabilitySummary {
            total_active: active.tasks.len(),
            candidates_total,
            runnable_candidates,
            blocked_by_dependencies: blocked_by_deps,
            blocked_by_schedule,
            blocked_by_status_or_flags: blocked_by_status,
        },
        tasks,
    })
}

/// Analyze a single task's runnability.
fn analyze_task_runnability(
    task: &Task,
    active: &QueueFile,
    done: Option<&QueueFile>,
    now_rfc3339: &str,
    now_dt: time::OffsetDateTime,
    options: RunnableSelectionOptions,
) -> TaskRunnabilityRow {
    let mut reasons = Vec::new();
    let mut runnable = true;

    // 1. Status/flags check
    match task.status {
        TaskStatus::Done | TaskStatus::Rejected => {
            runnable = false;
            reasons.push(NotRunnableReason::StatusNotRunnable {
                status: task.status,
            });
        }
        TaskStatus::Draft => {
            if !options.include_draft {
                runnable = false;
                reasons.push(NotRunnableReason::DraftExcluded);
            }
        }
        TaskStatus::Todo | TaskStatus::Doing => {}
    }

    // 2. Dependency check (only if not already blocked by status)
    if runnable || reasons.is_empty() {
        let dep_issues = check_dependencies(task, active, done);
        if !dep_issues.is_empty() {
            runnable = false;
            reasons.push(NotRunnableReason::UnmetDependencies {
                dependencies: dep_issues,
            });
        }
    }

    // 3. Schedule check (only if not already blocked)
    if (runnable
        || reasons
            .iter()
            .all(|r| !matches!(r, NotRunnableReason::StatusNotRunnable { .. })))
        && let Some(ref scheduled) = task.scheduled_start
        && let Ok(scheduled_dt) = crate::timeutil::parse_rfc3339(scheduled)
        && scheduled_dt > now_dt
    {
        runnable = false;
        let seconds_until = (scheduled_dt - now_dt).whole_seconds();
        reasons.push(NotRunnableReason::ScheduledStartInFuture {
            scheduled_start: scheduled.clone(),
            now: now_rfc3339.to_string(),
            seconds_until_runnable: seconds_until,
        });
    }

    TaskRunnabilityRow {
        id: task.id.clone(),
        status: task.status,
        runnable,
        reasons,
    }
}

/// Check dependencies and return issues.
fn check_dependencies(
    task: &Task,
    active: &QueueFile,
    done: Option<&QueueFile>,
) -> Vec<DependencyIssue> {
    let mut issues = Vec::new();

    for dep_id in &task.depends_on {
        let dep_task = find_task_across(active, done, dep_id);
        match dep_task {
            Some(t) => {
                if t.status != TaskStatus::Done && t.status != TaskStatus::Rejected {
                    issues.push(DependencyIssue::NotComplete {
                        id: dep_id.clone(),
                        status: t.status,
                    });
                }
            }
            None => {
                issues.push(DependencyIssue::Missing { id: dep_id.clone() });
            }
        }
    }

    issues
}

/// Check if a specific task is runnable (convenience wrapper).
pub fn is_task_runnable_detailed(
    task: &Task,
    active: &QueueFile,
    done: Option<&QueueFile>,
    now_rfc3339: &str,
    include_draft: bool,
) -> Result<(bool, Vec<NotRunnableReason>)> {
    let now_dt = crate::timeutil::parse_rfc3339(now_rfc3339)?;
    let options = RunnableSelectionOptions::new(include_draft, false);
    let row = analyze_task_runnability(task, active, done, now_rfc3339, now_dt, options);
    Ok((row.runnable, row.reasons))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use std::collections::HashMap;

    fn make_task(
        id: &str,
        status: TaskStatus,
        scheduled_start: Option<&str>,
        depends_on: Vec<&str>,
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
            scheduled_start: scheduled_start.map(|s| s.to_string()),
            depends_on: depends_on.into_iter().map(|s| s.to_string()).collect(),
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn test_runnable_todo_no_deps() {
        let tasks = vec![make_task("RQ-0001", TaskStatus::Todo, None, vec![])];
        let active = QueueFile { version: 1, tasks };
        let now = "2026-01-18T12:00:00Z";

        let report = queue_runnability_report_at(
            now,
            &active,
            None,
            RunnableSelectionOptions::new(false, false),
        )
        .unwrap();

        assert!(report.tasks[0].runnable);
        assert!(report.tasks[0].reasons.is_empty());
        assert_eq!(report.summary.runnable_candidates, 1);
    }

    #[test]
    fn test_blocked_by_missing_dependency() {
        let tasks = vec![make_task(
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

        assert!(!report.tasks[0].runnable);
        assert_eq!(report.tasks[0].reasons.len(), 1);
        assert!(matches!(
            &report.tasks[0].reasons[0],
            NotRunnableReason::UnmetDependencies { dependencies } if matches!(
                &dependencies[0],
                DependencyIssue::Missing { id } if id == "RQ-0002"
            )
        ));
        assert_eq!(report.summary.blocked_by_dependencies, 1);
    }

    #[test]
    fn test_blocked_by_incomplete_dependency() {
        let tasks = vec![
            make_task("RQ-0001", TaskStatus::Todo, None, vec!["RQ-0002"]),
            make_task("RQ-0002", TaskStatus::Todo, None, vec![]),
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
        assert!(matches!(
            &report.tasks[0].reasons[0],
            NotRunnableReason::UnmetDependencies { dependencies } if matches!(
                &dependencies[0],
                DependencyIssue::NotComplete { id, status } if id == "RQ-0002" && *status == TaskStatus::Todo
            )
        ));
    }

    #[test]
    fn test_blocked_by_future_schedule() {
        let tasks = vec![make_task(
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
        assert!(matches!(
            &report.tasks[0].reasons[0],
            NotRunnableReason::ScheduledStartInFuture { scheduled_start, .. } if scheduled_start == "2026-12-31T00:00:00Z"
        ));
        assert_eq!(report.summary.blocked_by_schedule, 1);
    }

    #[test]
    fn test_blocked_by_both_deps_and_schedule() {
        let tasks = vec![make_task(
            "RQ-0001",
            TaskStatus::Todo,
            Some("2026-12-31T00:00:00Z"),
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

        assert!(!report.tasks[0].runnable);
        assert_eq!(report.tasks[0].reasons.len(), 2);
        // Order: deps first, then schedule
        assert!(matches!(
            report.tasks[0].reasons[0],
            NotRunnableReason::UnmetDependencies { .. }
        ));
        assert!(matches!(
            report.tasks[0].reasons[1],
            NotRunnableReason::ScheduledStartInFuture { .. }
        ));
    }

    #[test]
    fn test_draft_excluded_without_flag() {
        let tasks = vec![make_task("RQ-0001", TaskStatus::Draft, None, vec![])];
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
        // DraftExcluded is not counted in summary because draft tasks are not candidates
        // when include_draft=false
        assert_eq!(report.summary.blocked_by_status_or_flags, 0);
        assert_eq!(report.summary.candidates_total, 0);
    }

    #[test]
    fn test_draft_included_with_flag() {
        let tasks = vec![make_task("RQ-0001", TaskStatus::Draft, None, vec![])];
        let active = QueueFile { version: 1, tasks };
        let now = "2026-01-18T12:00:00Z";

        let report = queue_runnability_report_at(
            now,
            &active,
            None,
            RunnableSelectionOptions::new(true, false),
        )
        .unwrap();

        assert!(report.tasks[0].runnable);
        assert!(report.tasks[0].reasons.is_empty());
    }

    #[test]
    fn test_draft_blocked_by_schedule_even_with_flag() {
        let tasks = vec![make_task(
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

        assert!(!report.tasks[0].runnable);
        assert!(matches!(
            report.tasks[0].reasons[0],
            NotRunnableReason::ScheduledStartInFuture { .. }
        ));
    }

    #[test]
    fn test_done_status_not_runnable() {
        let tasks = vec![make_task("RQ-0001", TaskStatus::Done, None, vec![])];
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
    fn test_rejected_dependency_is_ok() {
        let done_tasks = vec![make_task("RQ-0002", TaskStatus::Rejected, None, vec![])];
        let done = QueueFile {
            version: 1,
            tasks: done_tasks,
        };
        let active_tasks = vec![make_task(
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
    fn test_done_dependency_is_ok() {
        let done_tasks = vec![make_task("RQ-0002", TaskStatus::Done, None, vec![])];
        let done = QueueFile {
            version: 1,
            tasks: done_tasks,
        };
        let active_tasks = vec![make_task(
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
    fn test_prefer_doing_selects_doing_task() {
        let tasks = vec![
            make_task("RQ-0001", TaskStatus::Todo, None, vec![]),
            make_task("RQ-0002", TaskStatus::Doing, None, vec![]),
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
    fn test_no_prefer_doing_skips_doing() {
        let tasks = vec![
            make_task("RQ-0001", TaskStatus::Todo, None, vec![]),
            make_task("RQ-0002", TaskStatus::Doing, None, vec![]),
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

        // Without prefer_doing, selects first runnable Todo
        assert_eq!(
            report.selection.selected_task_id,
            Some("RQ-0001".to_string())
        );
        assert_eq!(
            report.selection.selected_task_status,
            Some(TaskStatus::Todo)
        );
    }

    #[test]
    fn test_summary_counts_correctly() {
        let tasks = vec![
            make_task("RQ-0001", TaskStatus::Todo, None, vec![]), // runnable
            make_task(
                "RQ-0002",
                TaskStatus::Todo,
                Some("2026-12-31T00:00:00Z"),
                vec![],
            ), // blocked by schedule
            make_task("RQ-0003", TaskStatus::Todo, None, vec!["RQ-0009"]), // blocked by deps
            make_task("RQ-0004", TaskStatus::Draft, None, vec![]), // not a candidate (draft excluded)
            make_task("RQ-0005", TaskStatus::Done, None, vec![]),  // not a candidate (done)
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

        assert_eq!(report.summary.total_active, 5);
        assert_eq!(report.summary.candidates_total, 3); // Todo only (RQ-0001, 2, 3)
        assert_eq!(report.summary.runnable_candidates, 1);
        assert_eq!(report.summary.blocked_by_dependencies, 1);
        assert_eq!(report.summary.blocked_by_schedule, 1);
        assert_eq!(report.summary.blocked_by_status_or_flags, 0); // Draft is not a candidate
    }

    #[test]
    fn test_report_version_is_stable() {
        let tasks = vec![make_task("RQ-0001", TaskStatus::Todo, None, vec![])];
        let active = QueueFile { version: 1, tasks };
        let now = "2026-01-18T12:00:00Z";

        let report = queue_runnability_report_at(
            now,
            &active,
            None,
            RunnableSelectionOptions::new(false, false),
        )
        .unwrap();

        assert_eq!(report.version, RUNNABILITY_REPORT_VERSION);
    }

    #[test]
    fn test_json_serialization_roundtrip() {
        let tasks = vec![
            make_task("RQ-0001", TaskStatus::Todo, None, vec!["RQ-0002"]),
            make_task(
                "RQ-0002",
                TaskStatus::Todo,
                Some("2026-12-31T00:00:00Z"),
                vec![],
            ),
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

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"version\":"));
        assert!(json.contains("\"now\":\"2026-01-18T12:00:00Z\""));
        assert!(json.contains("\"runnable\":false"));
    }
}
