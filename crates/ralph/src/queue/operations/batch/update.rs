//! Batch update operations for tasks.
//!
//! Responsibilities:
//! - Batch set status for multiple tasks.
//! - Batch set custom fields for multiple tasks.
//! - Batch apply task edits for multiple tasks.
//!
//! Does not handle:
//! - Task creation or deletion (see generate.rs and delete.rs).
//! - Task filtering/selection (see filters.rs).
//!
//! Assumptions/invariants:
//! - All operations support atomic mode (fail on first error) or continue-on-error mode.
//! - Task IDs are deduplicated before processing.

use crate::contracts::{QueueFile, TaskStatus};
use crate::queue;
use crate::queue::TaskEditKey;
use anyhow::{Result, bail};

use super::{BatchOperationResult, BatchTaskResult, deduplicate_task_ids, validate_task_ids_exist};

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
                    created_task_ids: Vec::new(),
                });
                succeeded += 1;
            }
            Err(e) => {
                let error_msg = e.to_string();
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: false,
                    error: Some(error_msg.clone()),
                    created_task_ids: Vec::new(),
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
                    created_task_ids: Vec::new(),
                });
                succeeded += 1;
            }
            Err(e) => {
                let error_msg = e.to_string();
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: false,
                    error: Some(error_msg.clone()),
                    created_task_ids: Vec::new(),
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
                    created_task_ids: Vec::new(),
                });
                succeeded += 1;
            }
            Err(e) => {
                let error_msg = e.to_string();
                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: false,
                    error: Some(error_msg.clone()),
                    created_task_ids: Vec::new(),
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
