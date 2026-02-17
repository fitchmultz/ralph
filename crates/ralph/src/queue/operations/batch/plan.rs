//! Batch plan manipulation operations.
//!
//! Responsibilities:
//! - Batch append plan items to multiple tasks.
//! - Batch prepend plan items to multiple tasks.
//!
//! Does not handle:
//! - Task creation or deletion.
//! - Task field updates other than plan and updated_at.
//! - Plan item distribution during task split (see generate.rs).
//!
//! Assumptions/invariants:
//! - Both append and prepend update the task's updated_at timestamp.
//! - Empty plan items lists are rejected before processing.

use crate::contracts::QueueFile;
use anyhow::{Result, bail};

use super::{
    BatchOperationResult, BatchResultCollector, preprocess_batch_ids, validate_task_ids_exist,
};

/// Batch append plan items to multiple tasks.
///
/// # Arguments
/// * `queue` - The queue file to modify
/// * `task_ids` - List of task IDs to update
/// * `plan_items` - Plan items to append
/// * `now_rfc3339` - Current timestamp for updated_at
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures.
pub fn batch_plan_append(
    queue: &mut QueueFile,
    task_ids: &[String],
    plan_items: &[String],
    now_rfc3339: &str,
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    let unique_ids = preprocess_batch_ids(task_ids, "plan append")?;

    if plan_items.is_empty() {
        bail!("No plan items provided for batch plan append");
    }

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut collector =
        BatchResultCollector::new(unique_ids.len(), continue_on_error, "plan append");

    for task_id in &unique_ids {
        match queue.tasks.iter_mut().find(|t| t.id == *task_id) {
            Some(task) => {
                task.plan.extend(plan_items.iter().cloned());
                task.updated_at = Some(now_rfc3339.to_string());
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

/// Batch prepend plan items to multiple tasks.
///
/// # Arguments
/// * `queue` - The queue file to modify
/// * `task_ids` - List of task IDs to update
/// * `plan_items` - Plan items to prepend
/// * `now_rfc3339` - Current timestamp for updated_at
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures.
pub fn batch_plan_prepend(
    queue: &mut QueueFile,
    task_ids: &[String],
    plan_items: &[String],
    now_rfc3339: &str,
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    let unique_ids = preprocess_batch_ids(task_ids, "plan prepend")?;

    if plan_items.is_empty() {
        bail!("No plan items provided for batch plan prepend");
    }

    // In atomic mode, validate all IDs exist first
    if !continue_on_error {
        validate_task_ids_exist(queue, &unique_ids)?;
    }

    let mut collector =
        BatchResultCollector::new(unique_ids.len(), continue_on_error, "plan prepend");

    for task_id in &unique_ids {
        match queue.tasks.iter_mut().find(|t| t.id == *task_id) {
            Some(task) => {
                // Prepend items: new items first, then existing
                let mut new_plan = plan_items.to_vec();
                new_plan.append(&mut task.plan);
                task.plan = new_plan;
                task.updated_at = Some(now_rfc3339.to_string());

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
