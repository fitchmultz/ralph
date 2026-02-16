//! Batch task operations for efficient multi-task updates.
//!
//! Responsibilities:
//! - Apply operations to multiple tasks atomically or with partial success.
//! - Filter tasks by tags, status, priority, scope, and age for batch selection.
//! - Batch delete, archive, clone, split, and plan operations.
//! - Provide detailed progress and error reporting.
//!
//! Does not handle:
//! - CLI argument parsing or user interaction.
//! - Individual task validation beyond what's in the single-task operations.
//! - Persistence to disk (callers save the queue after batch operations).
//!
//! Assumptions/invariants:
//! - Callers provide a loaded QueueFile and valid RFC3339 timestamp.
//! - Tag filtering is case-insensitive and OR-based (any tag matches).
//! - Status/priority/scope filters use OR logic within each filter type.
//! - Task IDs are unique within the queue.

mod delete;
mod display;
mod filters;
mod generate;
mod plan;
mod update;

pub use delete::{batch_archive_tasks, batch_delete_tasks};
pub use display::print_batch_results;
pub use filters::{
    BatchTaskFilters, filter_tasks_by_tags, parse_older_than_cutoff, resolve_task_ids,
    resolve_task_ids_filtered,
};
pub use generate::{batch_clone_tasks, batch_split_tasks};
pub use plan::{batch_plan_append, batch_plan_prepend};
pub use update::{batch_apply_edit, batch_set_field, batch_set_status};

/// Result of a batch operation on a single task.
#[derive(Debug, Clone)]
pub struct BatchTaskResult {
    pub task_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub created_task_ids: Vec<String>,
}

/// Overall result of a batch operation.
#[derive(Debug, Clone)]
pub struct BatchOperationResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<BatchTaskResult>,
}

impl BatchOperationResult {
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0
    }

    pub fn has_failures(&self) -> bool {
        self.failed > 0
    }
}

/// Collect unique task IDs from a list of tasks.
pub fn collect_task_ids(tasks: &[&crate::contracts::Task]) -> Vec<String> {
    tasks.iter().map(|t| t.id.clone()).collect()
}

/// Deduplicate task IDs while preserving order.
pub(crate) fn deduplicate_task_ids(task_ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for id in task_ids {
        let trimmed = id.trim().to_string();
        if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
            result.push(trimmed);
        }
    }
    result
}

/// Validate that all task IDs exist in the queue.
///
/// Returns an error if any task ID is not found.
pub(crate) fn validate_task_ids_exist(
    queue: &crate::contracts::QueueFile,
    task_ids: &[String],
) -> anyhow::Result<()> {
    use anyhow::bail;

    for task_id in task_ids {
        let needle = task_id.trim();
        if needle.is_empty() {
            bail!("Empty task ID provided");
        }
        if !queue.tasks.iter().any(|t| t.id.trim() == needle) {
            bail!("Task not found: {}", needle);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        batch_delete_tasks, batch_plan_append, batch_plan_prepend, parse_older_than_cutoff,
    };
    use crate::contracts::{QueueFile, Task};

    #[test]
    fn parse_older_than_cutoff_parses_days() {
        let now = "2026-02-05T00:00:00Z";
        let result = parse_older_than_cutoff(now, "7d").unwrap();
        assert!(result.contains("2026-01-29"));
    }

    #[test]
    fn parse_older_than_cutoff_parses_weeks() {
        let now = "2026-02-05T00:00:00Z";
        let result = parse_older_than_cutoff(now, "2w").unwrap();
        assert!(result.contains("2026-01-22"));
    }

    #[test]
    fn parse_older_than_cutoff_parses_date() {
        let result = parse_older_than_cutoff("2026-02-05T00:00:00Z", "2026-01-01").unwrap();
        assert!(result.contains("2026-01-01"));
    }

    #[test]
    fn parse_older_than_cutoff_parses_rfc3339() {
        let result =
            parse_older_than_cutoff("2026-02-05T00:00:00Z", "2026-01-15T12:00:00Z").unwrap();
        assert!(result.contains("2026-01-15"));
    }

    #[test]
    fn parse_older_than_cutoff_rejects_invalid() {
        let result = parse_older_than_cutoff("2026-02-05T00:00:00Z", "invalid");
        assert!(result.is_err());
    }

    #[test]
    fn batch_delete_tasks_removes_tasks() {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![
                Task {
                    id: "RQ-0001".to_string(),
                    title: "Task 1".to_string(),
                    ..Default::default()
                },
                Task {
                    id: "RQ-0002".to_string(),
                    title: "Task 2".to_string(),
                    ..Default::default()
                },
                Task {
                    id: "RQ-0003".to_string(),
                    title: "Task 3".to_string(),
                    ..Default::default()
                },
            ],
        };

        let result = batch_delete_tasks(
            &mut queue,
            &["RQ-0001".to_string(), "RQ-0002".to_string()],
            false,
        )
        .unwrap();

        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 0);
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0003");
    }

    #[test]
    fn batch_delete_tasks_atomic_fails_on_missing() {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![Task {
                id: "RQ-0001".to_string(),
                title: "Task 1".to_string(),
                ..Default::default()
            }],
        };

        let result = batch_delete_tasks(
            &mut queue,
            &["RQ-0001".to_string(), "RQ-9999".to_string()],
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn batch_plan_append_adds_items() {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![Task {
                id: "RQ-0001".to_string(),
                title: "Task 1".to_string(),
                plan: vec!["Step 1".to_string()],
                ..Default::default()
            }],
        };

        let result = batch_plan_append(
            &mut queue,
            &["RQ-0001".to_string()],
            &["Step 2".to_string(), "Step 3".to_string()],
            "2026-02-05T00:00:00Z",
            false,
        )
        .unwrap();

        assert_eq!(result.succeeded, 1);
        assert_eq!(queue.tasks[0].plan.len(), 3);
        assert_eq!(queue.tasks[0].plan[0], "Step 1");
        assert_eq!(queue.tasks[0].plan[1], "Step 2");
        assert_eq!(queue.tasks[0].plan[2], "Step 3");
    }

    #[test]
    fn batch_plan_prepend_adds_items_first() {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![Task {
                id: "RQ-0001".to_string(),
                title: "Task 1".to_string(),
                plan: vec!["Step 2".to_string()],
                ..Default::default()
            }],
        };

        let result = batch_plan_prepend(
            &mut queue,
            &["RQ-0001".to_string()],
            &["Step 1".to_string()],
            "2026-02-05T00:00:00Z",
            false,
        )
        .unwrap();

        assert_eq!(result.succeeded, 1);
        assert_eq!(queue.tasks[0].plan.len(), 2);
        assert_eq!(queue.tasks[0].plan[0], "Step 1");
        assert_eq!(queue.tasks[0].plan[1], "Step 2");
    }
}
