//! Batch update operations for tasks.
//!
//! Purpose:
//! - Batch update operations for tasks.
//!
//! Responsibilities:
//! - Batch set status for multiple tasks.
//! - Batch set custom fields for multiple tasks.
//! - Batch apply task edits for multiple tasks.
//!
//! Non-scope:
//! - Task creation or deletion (see generate.rs and delete.rs).
//! - Task filtering/selection (see filters.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - All operations support atomic mode (fail on first error) or continue-on-error mode.
//! - Task IDs are deduplicated before processing.

use crate::contracts::{QueueFile, TaskStatus};
use crate::queue;
use crate::queue::TaskEditKey;
use anyhow::Result;

use super::{
    BatchOperationResult, BatchResultCollector, preprocess_batch_ids, validate_task_ids_exist,
};

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
    let unique_ids = preprocess_batch_ids(task_ids, "status update")?;

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut collector =
        BatchResultCollector::new(unique_ids.len(), continue_on_error, "status update");

    for task_id in &unique_ids {
        match queue::set_status(queue, task_id, status, now_rfc3339, note) {
            Ok(()) => {
                collector.record_success(task_id.clone(), Vec::new());
            }
            Err(e) => {
                let error_msg = e.to_string();
                collector.record_failure(task_id.clone(), error_msg)?;
            }
        }
    }

    Ok(collector.finish())
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
    let unique_ids = preprocess_batch_ids(task_ids, "field update")?;

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut collector =
        BatchResultCollector::new(unique_ids.len(), continue_on_error, "field update");

    for task_id in &unique_ids {
        match queue::set_field(queue, task_id, key, value, now_rfc3339) {
            Ok(()) => {
                collector.record_success(task_id.clone(), Vec::new());
            }
            Err(e) => {
                let error_msg = e.to_string();
                collector.record_failure(task_id.clone(), error_msg)?;
            }
        }
    }

    Ok(collector.finish())
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
    let unique_ids = preprocess_batch_ids(task_ids, "edit")?;

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut collector = BatchResultCollector::new(unique_ids.len(), continue_on_error, "edit");

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
                collector.record_success(task_id.clone(), Vec::new());
            }
            Err(e) => {
                let error_msg = e.to_string();
                collector.record_failure(task_id.clone(), error_msg)?;
            }
        }
    }

    Ok(collector.finish())
}
