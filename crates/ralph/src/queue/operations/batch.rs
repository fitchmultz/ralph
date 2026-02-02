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
use anyhow::{Result, bail};

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
