//! Batch task operations for efficient multi-task updates.
//!
//! Responsibilities:
//! - Apply operations to multiple tasks atomically or with partial success.
//! - Filter tasks by tags for batch selection.
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
//! - Task IDs are unique within the queue.

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue;
use crate::queue::TaskEditKey;
use anyhow::{bail, Result};

/// Result of a batch operation on a single task.
#[derive(Debug, Clone)]
pub struct BatchTaskResult {
    pub task_id: String,
    pub success: bool,
    pub error: Option<String>,
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

/// Filter tasks by tags (case-insensitive, OR-based).
///
/// Returns tasks where ANY of the task's tags match ANY of the filter tags (case-insensitive).
pub fn filter_tasks_by_tags<'a>(queue: &'a QueueFile, tags: &[String]) -> Vec<&'a Task> {
    if tags.is_empty() {
        return Vec::new();
    }

    let normalized_filter_tags: Vec<String> = tags
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    queue
        .tasks
        .iter()
        .filter(|task| {
            task.tags.iter().any(|task_tag| {
                let normalized_task_tag = task_tag.trim().to_lowercase();
                normalized_filter_tags
                    .iter()
                    .any(|filter_tag| filter_tag == &normalized_task_tag)
            })
        })
        .collect()
}

/// Collect unique task IDs from a list of tasks.
pub fn collect_task_ids(tasks: &[&Task]) -> Vec<String> {
    tasks.iter().map(|t| t.id.clone()).collect()
}

/// Validate that all task IDs exist in the queue.
///
/// Returns an error if any task ID is not found.
fn validate_task_ids_exist(queue: &QueueFile, task_ids: &[String]) -> Result<()> {
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

/// Deduplicate task IDs while preserving order.
fn deduplicate_task_ids(task_ids: &[String]) -> Vec<String> {
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

/// Batch set status for multiple tasks.
///
/// # Arguments
/// * `queue` - The queue file to modify
/// * `task_ids` - List of task IDs to update
/// * `status` - The new status to set
/// * `now_rfc3339` - Current timestamp for updated_at
/// * `note` - Optional note to append to each task
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures.
pub fn batch_set_status(
    queue: &mut QueueFile,
    task_ids: &[String],
    status: TaskStatus,
    now_rfc3339: &str,
    note: Option<&str>,
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    let unique_ids = deduplicate_task_ids(task_ids);

    if unique_ids.is_empty() {
        bail!("No task IDs provided for batch status update");
    }

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut results = Vec::new();
    let mut succeeded = 0;
    let mut failed = 0;

    for task_id in &unique_ids {
        match queue::set_status(queue, task_id, status, now_rfc3339, note) {
            Ok(()) => {
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: true,
                    error: None,
                });
                succeeded += 1;
            }
            Err(e) => {
                let error_msg = e.to_string();
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: false,
                    error: Some(error_msg.clone()),
                });
                failed += 1;

                if !continue_on_error {
                    // In atomic mode, we should have already validated, but just in case
                    bail!(
                        "Batch operation failed at task {}: {}. Use --continue-on-error to process remaining tasks.",
                        task_id,
                        error_msg
                    );
                }
            }
        }
    }

    Ok(BatchOperationResult {
        total: unique_ids.len(),
        succeeded,
        failed,
        results,
    })
}

/// Batch set custom field for multiple tasks.
///
/// # Arguments
/// * `queue` - The queue file to modify
/// * `task_ids` - List of task IDs to update
/// * `key` - The custom field key
/// * `value` - The custom field value
/// * `now_rfc3339` - Current timestamp for updated_at
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures.
pub fn batch_set_field(
    queue: &mut QueueFile,
    task_ids: &[String],
    key: &str,
    value: &str,
    now_rfc3339: &str,
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    let unique_ids = deduplicate_task_ids(task_ids);

    if unique_ids.is_empty() {
        bail!("No task IDs provided for batch field update");
    }

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut results = Vec::new();
    let mut succeeded = 0;
    let mut failed = 0;

    for task_id in &unique_ids {
        match queue::set_field(queue, task_id, key, value, now_rfc3339) {
            Ok(()) => {
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: true,
                    error: None,
                });
                succeeded += 1;
            }
            Err(e) => {
                let error_msg = e.to_string();
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: false,
                    error: Some(error_msg.clone()),
                });
                failed += 1;

                if !continue_on_error {
                    bail!(
                        "Batch operation failed at task {}: {}. Use --continue-on-error to process remaining tasks.",
                        task_id,
                        error_msg
                    );
                }
            }
        }
    }

    Ok(BatchOperationResult {
        total: unique_ids.len(),
        succeeded,
        failed,
        results,
    })
}

/// Batch edit field for multiple tasks.
///
/// # Arguments
/// * `queue` - The queue file to modify
/// * `done` - Optional done file for validation
/// * `task_ids` - List of task IDs to update
/// * `key` - The field to edit
/// * `value` - The new value
/// * `now_rfc3339` - Current timestamp for updated_at
/// * `id_prefix` - Task ID prefix for validation
/// * `id_width` - Task ID width for validation
/// * `max_dependency_depth` - Maximum dependency depth for validation
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures.
#[allow(clippy::too_many_arguments)]
pub fn batch_apply_edit(
    queue: &mut QueueFile,
    done: Option<&QueueFile>,
    task_ids: &[String],
    key: TaskEditKey,
    value: &str,
    now_rfc3339: &str,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    let unique_ids = deduplicate_task_ids(task_ids);

    if unique_ids.is_empty() {
        bail!("No task IDs provided for batch edit");
    }

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut results = Vec::new();
    let mut succeeded = 0;
    let mut failed = 0;

    for task_id in &unique_ids {
        match queue::apply_task_edit(
            queue,
            done,
            task_id,
            key,
            value,
            now_rfc3339,
            id_prefix,
            id_width,
            max_dependency_depth,
        ) {
            Ok(()) => {
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: true,
                    error: None,
                });
                succeeded += 1;
            }
            Err(e) => {
                let error_msg = e.to_string();
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: false,
                    error: Some(error_msg.clone()),
                });
                failed += 1;

                if !continue_on_error {
                    bail!(
                        "Batch operation failed at task {}: {}. Use --continue-on-error to process remaining tasks.",
                        task_id,
                        error_msg
                    );
                }
            }
        }
    }

    Ok(BatchOperationResult {
        total: unique_ids.len(),
        succeeded,
        failed,
        results,
    })
}

/// Resolve task IDs from either explicit list or tag filter.
///
/// If `tag_filter` is provided, returns tasks matching any of the tags.
/// Otherwise, returns the explicit task IDs (after deduplication).
///
/// # Arguments
/// * `queue` - The queue file to search
/// * `task_ids` - Explicit list of task IDs
/// * `tag_filter` - Optional list of tags to filter by
///
/// # Returns
/// A deduplicated list of task IDs to operate on.
pub fn resolve_task_ids(
    queue: &QueueFile,
    task_ids: &[String],
    tag_filter: &[String],
) -> Result<Vec<String>> {
    // If tag filter is provided, use it to select tasks
    if !tag_filter.is_empty() {
        let matching_tasks = filter_tasks_by_tags(queue, tag_filter);
        if matching_tasks.is_empty() {
            let tags_str = tag_filter.join(", ");
            bail!("No tasks found with tags: {}", tags_str);
        }
        return Ok(collect_task_ids(&matching_tasks));
    }

    // Otherwise, use explicit task IDs
    let unique_ids = deduplicate_task_ids(task_ids);
    if unique_ids.is_empty() {
        bail!("No task IDs provided. Provide task IDs or use --tag-filter to select tasks by tag.");
    }

    Ok(unique_ids)
}

/// Print batch operation results in a user-friendly format.
pub fn print_batch_results(result: &BatchOperationResult, operation_name: &str, dry_run: bool) {
    if dry_run {
        println!(
            "Dry run - would perform {} on {} tasks:",
            operation_name, result.total
        );
        for r in &result.results {
            if r.success {
                println!("  - {}: would update", r.task_id);
            } else {
                println!(
                    "  - {}: would fail - {}",
                    r.task_id,
                    r.error.as_deref().unwrap_or("unknown error")
                );
            }
        }
        println!("Dry run complete. No changes made.");
        return;
    }

    if result.has_failures() {
        println!("{} completed with errors:", operation_name);
        for r in &result.results {
            if r.success {
                println!("  ✓ {}: updated", r.task_id);
            } else {
                println!(
                    "  ✗ {}: failed - {}",
                    r.task_id,
                    r.error.as_deref().unwrap_or("unknown error")
                );
            }
        }
        println!(
            "Completed: {}/{} tasks updated successfully.",
            result.succeeded, result.total
        );
    } else {
        println!("{} completed successfully:", operation_name);
        for r in &result.results {
            println!("  ✓ {}", r.task_id);
        }
        println!("Updated {} tasks.", result.succeeded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
    use std::collections::HashMap;

    fn test_task_with_tags(id: &str, tags: Vec<&str>) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Test task {}", id),
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            created_at: Some("2026-01-20T12:00:00Z".to_string()),
            updated_at: Some("2026-01-20T12:00:00Z".to_string()),
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            agent: None,
        }
    }

    fn test_queue_with_tasks(tasks: Vec<Task>) -> QueueFile {
        QueueFile { version: 1, tasks }
    }

    #[test]
    fn filter_tasks_by_tags_matches_case_insensitive() {
        let tasks = vec![
            test_task_with_tags("RQ-0001", vec!["rust", "cli"]),
            test_task_with_tags("RQ-0002", vec!["Rust", "backend"]),
            test_task_with_tags("RQ-0003", vec!["python"]),
        ];
        let queue = test_queue_with_tasks(tasks);

        let result = filter_tasks_by_tags(&queue, &["rust".to_string()]);

        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|t| t.id == "RQ-0001"));
        assert!(result.iter().any(|t| t.id == "RQ-0002"));
    }

    #[test]
    fn filter_tasks_by_tags_uses_or_logic() {
        let tasks = vec![
            test_task_with_tags("RQ-0001", vec!["rust"]),
            test_task_with_tags("RQ-0002", vec!["cli"]),
            test_task_with_tags("RQ-0003", vec!["python"]),
        ];
        let queue = test_queue_with_tasks(tasks);

        let result = filter_tasks_by_tags(&queue, &["rust".to_string(), "cli".to_string()]);

        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|t| t.id == "RQ-0001"));
        assert!(result.iter().any(|t| t.id == "RQ-0002"));
    }

    #[test]
    fn filter_tasks_by_tags_empty_filter_returns_empty() {
        let tasks = vec![test_task_with_tags("RQ-0001", vec!["rust"])];
        let queue = test_queue_with_tasks(tasks);

        let result = filter_tasks_by_tags(&queue, &[]);

        assert!(result.is_empty());
    }

    #[test]
    fn filter_tasks_by_tags_no_match_returns_empty() {
        let tasks = vec![test_task_with_tags("RQ-0001", vec!["rust"])];
        let queue = test_queue_with_tasks(tasks);

        let result = filter_tasks_by_tags(&queue, &["python".to_string()]);

        assert!(result.is_empty());
    }

    #[test]
    fn deduplicate_task_ids_preserves_order() {
        let ids = vec![
            "RQ-0001".to_string(),
            "RQ-0002".to_string(),
            "RQ-0001".to_string(),
            "RQ-0003".to_string(),
            "RQ-0002".to_string(),
        ];

        let result = deduplicate_task_ids(&ids);

        assert_eq!(result, vec!["RQ-0001", "RQ-0002", "RQ-0003"]);
    }

    #[test]
    fn deduplicate_task_ids_skips_empty() {
        let ids = vec![
            "RQ-0001".to_string(),
            "".to_string(),
            "RQ-0002".to_string(),
            " ".to_string(),
        ];

        let result = deduplicate_task_ids(&ids);

        assert_eq!(result, vec!["RQ-0001", "RQ-0002"]);
    }

    #[test]
    fn batch_set_status_updates_all_tasks() {
        let tasks = vec![
            test_task_with_tags("RQ-0001", vec![]),
            test_task_with_tags("RQ-0002", vec![]),
        ];
        let mut queue = test_queue_with_tasks(tasks);

        let result = batch_set_status(
            &mut queue,
            &["RQ-0001".to_string(), "RQ-0002".to_string()],
            TaskStatus::Doing,
            "2026-01-21T12:00:00Z",
            None,
            false,
        )
        .expect("batch operation should succeed");

        assert_eq!(result.total, 2);
        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 0);
        assert!(result.all_succeeded());

        // Verify tasks were updated
        assert_eq!(queue.tasks[0].status, TaskStatus::Doing);
        assert_eq!(queue.tasks[1].status, TaskStatus::Doing);
    }

    #[test]
    fn batch_set_status_atomic_fails_on_missing_task() {
        let tasks = vec![test_task_with_tags("RQ-0001", vec![])];
        let mut queue = test_queue_with_tasks(tasks);

        let result = batch_set_status(
            &mut queue,
            &["RQ-0001".to_string(), "RQ-9999".to_string()],
            TaskStatus::Doing,
            "2026-01-21T12:00:00Z",
            None,
            false,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("RQ-9999"));
        assert!(err.contains("not found"));

        // Verify no tasks were updated (atomic)
        assert_eq!(queue.tasks[0].status, TaskStatus::Todo);
    }

    #[test]
    fn batch_set_status_continue_on_error_reports_partial() {
        let tasks = vec![
            test_task_with_tags("RQ-0001", vec![]),
            test_task_with_tags("RQ-0002", vec![]),
        ];
        let mut queue = test_queue_with_tasks(tasks);

        let result = batch_set_status(
            &mut queue,
            &[
                "RQ-0001".to_string(),
                "RQ-9999".to_string(),
                "RQ-0002".to_string(),
            ],
            TaskStatus::Doing,
            "2026-01-21T12:00:00Z",
            None,
            true, // continue_on_error
        )
        .expect("batch operation should complete with partial success");

        assert_eq!(result.total, 3);
        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 1);
        assert!(result.has_failures());

        // Verify valid tasks were updated
        assert_eq!(queue.tasks[0].status, TaskStatus::Doing);
        assert_eq!(queue.tasks[1].status, TaskStatus::Doing);
    }

    #[test]
    fn batch_set_field_updates_custom_fields() {
        let tasks = vec![
            test_task_with_tags("RQ-0001", vec![]),
            test_task_with_tags("RQ-0002", vec![]),
        ];
        let mut queue = test_queue_with_tasks(tasks);

        let result = batch_set_field(
            &mut queue,
            &["RQ-0001".to_string(), "RQ-0002".to_string()],
            "severity",
            "high",
            "2026-01-21T12:00:00Z",
            false,
        )
        .expect("batch operation should succeed");

        assert_eq!(result.total, 2);
        assert_eq!(result.succeeded, 2);

        // Verify fields were set
        assert_eq!(
            queue.tasks[0].custom_fields.get("severity"),
            Some(&"high".to_string())
        );
        assert_eq!(
            queue.tasks[1].custom_fields.get("severity"),
            Some(&"high".to_string())
        );
    }

    #[test]
    fn resolve_task_ids_prefers_tag_filter() {
        let tasks = vec![
            test_task_with_tags("RQ-0001", vec!["rust"]),
            test_task_with_tags("RQ-0002", vec!["rust"]),
            test_task_with_tags("RQ-0003", vec!["python"]),
        ];
        let queue = test_queue_with_tasks(tasks);

        let result = resolve_task_ids(
            &queue,
            &["RQ-0003".to_string()], // Should be ignored
            &["rust".to_string()],
        )
        .expect("should resolve tasks");

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"RQ-0001".to_string()));
        assert!(result.contains(&"RQ-0002".to_string()));
    }

    #[test]
    fn resolve_task_ids_uses_explicit_ids_when_no_tag_filter() {
        let tasks = vec![
            test_task_with_tags("RQ-0001", vec![]),
            test_task_with_tags("RQ-0002", vec![]),
        ];
        let queue = test_queue_with_tasks(tasks);

        let result =
            resolve_task_ids(&queue, &["RQ-0001".to_string()], &[]).expect("should resolve tasks");

        assert_eq!(result, vec!["RQ-0001"]);
    }

    #[test]
    fn resolve_task_ids_errors_on_empty_input() {
        let tasks = vec![test_task_with_tags("RQ-0001", vec![])];
        let queue = test_queue_with_tasks(tasks);

        let result = resolve_task_ids(&queue, &[], &[]);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No task IDs provided"));
    }

    #[test]
    fn resolve_task_ids_errors_on_no_matching_tags() {
        let tasks = vec![test_task_with_tags("RQ-0001", vec!["rust"])];
        let queue = test_queue_with_tasks(tasks);

        let result = resolve_task_ids(&queue, &[], &["python".to_string()]);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No tasks found with tags"));
    }
}
