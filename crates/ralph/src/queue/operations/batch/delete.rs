//! Batch delete and archive operations for tasks.
//!
//! Purpose:
//! - Batch delete and archive operations for tasks.
//!
//! Responsibilities:
//! - Batch delete multiple tasks from the queue.
//! - Batch archive terminal tasks (Done/Rejected) from active queue to done.
//!
//! Non-scope:
//! - Task filtering/selection (see filters.rs).
//! - Task updates or field modifications (see update.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Archive only works on terminal tasks (Done or Rejected status).
//! - Delete permanently removes tasks without archival.

use crate::contracts::{QueueFile, TaskStatus};
use anyhow::Result;

use super::{
    BatchOperationResult, BatchResultCollector, preprocess_batch_ids, validate_task_ids_exist,
};

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
    let unique_ids = preprocess_batch_ids(task_ids, "delete")?;

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut collector = BatchResultCollector::new(unique_ids.len(), continue_on_error, "delete");

    for task_id in &unique_ids {
        match queue.tasks.iter().position(|t| t.id == *task_id) {
            Some(idx) => {
                queue.tasks.remove(idx);
                collector.record_success(task_id.clone(), Vec::new());
            }
            None => {
                collector.record_failure(
                    task_id.clone(),
                    crate::error_messages::task_not_found_batch_failure(task_id),
                )?;
            }
        }
    }

    Ok(collector.finish())
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
    let unique_ids = preprocess_batch_ids(task_ids, "archive")?;

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(active, &unique_ids)?;
    }

    let mut collector = BatchResultCollector::new(unique_ids.len(), continue_on_error, "archive");

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
                    collector.record_failure(task_id.clone(), err_msg)?;
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

                collector.record_success(task_id.clone(), Vec::new());
            }
            None => {
                collector.record_failure(
                    task_id.clone(),
                    crate::error_messages::task_not_found_in_queue(task_id),
                )?;
            }
        }
    }

    Ok(collector.finish())
}
