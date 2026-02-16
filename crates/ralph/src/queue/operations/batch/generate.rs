//! Batch task generation operations (clone and split).
//!
//! Responsibilities:
//! - Batch clone multiple tasks with new IDs.
//! - Batch split multiple tasks into child tasks.
//!
//! Does not handle:
//! - Task deletion (see delete.rs).
//! - Task filtering/selection (see filters.rs).
//! - Task field updates (see update.rs).
//!
//! Assumptions/invariants:
//! - Clone operations can source from active or done queues.
//! - Split operations only work on tasks in the active queue.
//! - Both operations support atomic rollback on failure.

use crate::contracts::{QueueFile, TaskStatus};
use crate::queue;
use crate::queue::operations::{CloneTaskOptions, SplitTaskOptions, suggest_new_task_insert_index};
use anyhow::{Result, bail};

use super::{BatchOperationResult, BatchTaskResult, deduplicate_task_ids};

/// Batch clone multiple tasks.
///
/// # Arguments
/// * `queue` - The active queue to insert cloned tasks into
/// * `done` - Optional done queue to search for source tasks
/// * `task_ids` - List of task IDs to clone
/// * `status` - Status for cloned tasks
/// * `title_prefix` - Optional prefix for cloned task titles
/// * `now_rfc3339` - Current timestamp for created_at/updated_at
/// * `id_prefix` - Task ID prefix
/// * `id_width` - Task ID numeric width
/// * `max_dependency_depth` - Max dependency depth for validation
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures, including created task IDs.
#[allow(clippy::too_many_arguments)]
pub fn batch_clone_tasks(
    queue: &mut QueueFile,
    done: Option<&QueueFile>,
    task_ids: &[String],
    status: TaskStatus,
    title_prefix: Option<&str>,
    now_rfc3339: &str,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    let unique_ids = deduplicate_task_ids(task_ids);

    if unique_ids.is_empty() {
        bail!("No task IDs provided for batch clone");
    }

    // In atomic mode, validate all source tasks exist first
    if !continue_on_error {
        for task_id in &unique_ids {
            let exists_in_active = queue.tasks.iter().any(|t| t.id == *task_id);
            let exists_in_done = done.is_some_and(|d| d.tasks.iter().any(|t| t.id == *task_id));
            if !exists_in_active && !exists_in_done {
                bail!("Source task not found: {}", task_id);
            }
        }
    }

    let mut results = Vec::new();
    let mut succeeded = 0;
    let mut failed = 0;

    // Create a working copy for atomic mode
    let original_queue = if !continue_on_error {
        Some(queue.clone())
    } else {
        None
    };

    for task_id in &unique_ids {
        let opts = CloneTaskOptions::new(task_id, status, now_rfc3339, id_prefix, id_width)
            .with_title_prefix(title_prefix)
            .with_max_depth(max_dependency_depth);

        match queue::operations::clone_task(queue, done, &opts) {
            Ok((new_id, cloned_task)) => {
                // Insert the cloned task at a good position
                let insert_idx = suggest_new_task_insert_index(queue);
                queue.tasks.insert(insert_idx, cloned_task);

                results.push(BatchTaskResult {
                    task_id: task_id.clone(),
                    success: true,
                    error: None,
                    created_task_ids: vec![new_id],
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
                    // Rollback: restore original queue
                    if let Some(ref original) = original_queue {
                        *queue = original.clone();
                    }
                    bail!(
                        "Batch clone failed at task {}: {}. Use --continue-on-error to process remaining tasks.",
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

/// Batch split multiple tasks into child tasks.
///
/// # Arguments
/// * `queue` - The active queue to modify
/// * `task_ids` - List of task IDs to split
/// * `number` - Number of child tasks to create per source
/// * `status` - Status for child tasks
/// * `title_prefix` - Optional prefix for child task titles
/// * `distribute_plan` - Whether to distribute plan items across children
/// * `now_rfc3339` - Current timestamp for timestamps
/// * `id_prefix` - Task ID prefix
/// * `id_width` - Task ID numeric width
/// * `max_dependency_depth` - Max dependency depth for validation
/// * `continue_on_error` - If true, continue processing on individual failures
///
/// # Returns
/// A `BatchOperationResult` with details of successes and failures.
#[allow(clippy::too_many_arguments)]
pub fn batch_split_tasks(
    queue: &mut QueueFile,
    task_ids: &[String],
    number: usize,
    status: TaskStatus,
    title_prefix: Option<&str>,
    distribute_plan: bool,
    now_rfc3339: &str,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
    continue_on_error: bool,
) -> Result<BatchOperationResult> {
    if number < 2 {
        bail!("Number of child tasks must be at least 2");
    }

    let unique_ids = deduplicate_task_ids(task_ids);

    if unique_ids.is_empty() {
        bail!("No task IDs provided for batch split");
    }

    // In atomic mode, validate all source tasks exist first
    if !continue_on_error {
        for task_id in &unique_ids {
            if !queue.tasks.iter().any(|t| t.id == *task_id) {
                bail!("Source task not found in active queue: {}", task_id);
            }
        }
    }

    let mut results = Vec::new();
    let mut succeeded = 0;
    let mut failed = 0;

    // Create a working copy for atomic mode
    let original_queue = if !continue_on_error {
        Some(queue.clone())
    } else {
        None
    };

    for task_id in &unique_ids {
        let opts = SplitTaskOptions::new(task_id, number, status, now_rfc3339, id_prefix, id_width)
            .with_title_prefix(title_prefix)
            .with_distribute_plan(distribute_plan)
            .with_max_depth(max_dependency_depth);

        match queue::operations::split_task(queue, None, &opts) {
            Ok((updated_source, child_tasks)) => {
                // Find source task position
                if let Some(idx) = queue.tasks.iter().position(|t| t.id == *task_id) {
                    // Replace source with updated version
                    queue.tasks[idx] = updated_source;

                    // Insert children after the source
                    let child_ids: Vec<String> = child_tasks.iter().map(|t| t.id.clone()).collect();
                    for (i, child) in child_tasks.into_iter().enumerate() {
                        queue.tasks.insert(idx + 1 + i, child);
                    }

                    results.push(BatchTaskResult {
                        task_id: task_id.clone(),
                        success: true,
                        error: None,
                        created_task_ids: child_ids,
                    });
                    succeeded += 1;
                } else {
                    // This shouldn't happen since we validated above
                    let err_msg = "Source task disappeared during split".to_string();
                    results.push(BatchTaskResult {
                        task_id: task_id.clone(),
                        success: false,
                        error: Some(err_msg.clone()),
                        created_task_ids: Vec::new(),
                    });
                    failed += 1;

                    if !continue_on_error {
                        if let Some(ref original) = original_queue {
                            *queue = original.clone();
                        }
                        bail!("{}", err_msg);
                    }
                }
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
                    if let Some(ref original) = original_queue {
                        *queue = original.clone();
                    }
                    bail!(
                        "Batch split failed at task {}: {}. Use --continue-on-error to process remaining tasks.",
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
