//! Batch delete and archive operations for tasks.
//!
//! Responsibilities:
//! - Batch delete multiple tasks from the queue.
//! - Batch archive terminal tasks (Done/Rejected) from active queue to done.
//!
//! Does not handle:
//! - Task filtering/selection (see filters.rs).
//! - Task updates or field modifications (see update.rs).
//!
//! Assumptions/invariants:
//! - Archive only works on terminal tasks (Done or Rejected status).
//! - Delete permanently removes tasks without archival.

use crate::contracts::{QueueFile, TaskStatus};
use anyhow::{Result, bail};

use super::{BatchOperationResult, BatchTaskResult, deduplicate_task_ids, validate_task_ids_exist};

/// Batch delete multiple tasks from the queue.
///
/// # Arguments
/// * `queue` - The queue file to modify
/// * `task_ids` - List of task IDs to delete
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures.
pub fn batch_delete_tasks(
    queue: &mut QueueFile,
    task_ids: &[String],
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    let unique_ids = deduplicate_task_ids(task_ids);

    if unique_ids.is_empty() {
        bail!("No task IDs provided for batch delete");
    }

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    // Build set of IDs to delete for O(1) lookup
    let ids_to_delete: std::collections::HashSet<String> = unique_ids.iter().cloned().collect();
    let _initial_count = queue.tasks.len();

    // Filter out tasks to delete
    let mut results = Vec::new();
    let mut succeeded = 0;
    let mut failed = 0;

    // First pass: validate all exist if atomic
    for task_id in &unique_ids {
        let exists = queue.tasks.iter().any(|t| t.id == *task_id);
        if !exists {
            results.push(BatchTaskResult {
                task_id: task_id.clone(),
                success: false,
                error: Some(format!("Task not found: {}", task_id)),
                created_task_ids: Vec::new(),
            });
            failed += 1;

            if !continue_on_error {
                bail!("Task not found: {}", task_id);
            }
        }
    }

    // Second pass: actually remove tasks (in reverse order to maintain indices if we used them)
    queue.tasks.retain(|task| {
        if ids_to_delete.contains(&task.id) {
            // Find the result entry for this task and mark success
            if let Some(_result) = results.iter_mut().find(|r| r.task_id == task.id) {
                // Already marked as failed, keep it that way
            } else {
                results.push(BatchTaskResult {
                    task_id: task.id.clone(),
                    success: true,
                    error: None,
                    created_task_ids: Vec::new(),
                });
                succeeded += 1;
            }
            false // Remove this task
        } else {
            true // Keep this task
        }
    });

    // Ensure we have results for all tasks
    for task_id in &unique_ids {
        if !results.iter().any(|r| r.task_id == *task_id) {
            results.push(BatchTaskResult {
                task_id: task_id.clone(),
                success: true,
                error: None,
                created_task_ids: Vec::new(),
            });
            succeeded += 1;
        }
    }

    Ok(BatchOperationResult {
        total: unique_ids.len(),
        succeeded,
        failed,
        results,
    })
}

/// Batch archive terminal tasks (Done/Rejected) from active queue to done.
///
/// # Arguments
/// * `active` - The active queue file to modify
/// * `done` - The done archive file to append to
/// * `task_ids` - List of task IDs to archive
/// * `now_rfc3339` - Current timestamp for updated_at/completed_at
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures.
pub fn batch_archive_tasks(
    active: &mut QueueFile,
    done: &mut QueueFile,
    task_ids: &[String],
    now_rfc3339: &str,
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    let unique_ids = deduplicate_task_ids(task_ids);

    if unique_ids.is_empty() {
        bail!("No task IDs provided for batch archive");
    }

    let mut results = Vec::new();
    let mut succeeded = 0;
    let mut failed = 0;

    for task_id in &unique_ids {
        // Find the task in active queue
        let task_idx = active.tasks.iter().position(|t| t.id == *task_id);

        match task_idx {
            Some(idx) => {
                let task = &active.tasks[idx];

                // Check if task is terminal (Done or Rejected)
                if !matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
                    let err_msg = format!(
                        "Task {} has status '{}' which is not terminal (Done/Rejected)",
                        task_id, task.status
                    );
                    results.push(BatchTaskResult {
                        task_id: task_id.clone(),
                        success: false,
                        error: Some(err_msg.clone()),
                        created_task_ids: Vec::new(),
                    });
                    failed += 1;

                    if !continue_on_error {
                        bail!("{}", err_msg);
                    }
                    continue;
                }

                // Remove task from active and add to done
                let mut task = active.tasks.remove(idx);

                // Stamp completed_at if missing
                if task.completed_at.is_none() || task.completed_at.as_ref().unwrap().is_empty() {
                    task.completed_at = Some(now_rfc3339.to_string());
                }
                task.updated_at = Some(now_rfc3339.to_string());

                done.tasks.push(task);

                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: true,
                    error: None,
                    created_task_ids: Vec::new(),
                });
                succeeded += 1;
            }
            None => {
                let err_msg = format!("Task not found in active queue: {}", task_id);
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: false,
                    error: Some(err_msg.clone()),
                    created_task_ids: Vec::new(),
                });
                failed += 1;

                if !continue_on_error {
                    bail!("{}", err_msg);
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
