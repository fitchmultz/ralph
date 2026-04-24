//! Batch operation validation and bookkeeping helpers.
//!
//! Purpose:
//! - Batch operation validation and bookkeeping helpers.
//!
//! Responsibilities:
//! - Collect and summarize per-task batch operation results.
//! - Normalize task ID lists before batch execution.
//! - Validate that referenced task IDs exist in the active queue.
//!
//! Non-scope:
//! - Applying batch mutations (handled by sibling batch modules).
//! - CLI argument parsing or result rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Task IDs are trimmed before deduplication and existence checks.
//! - Collector counts must stay synchronized with recorded results.
//! - Validation is performed against an already loaded `QueueFile`.

use crate::contracts::{QueueFile, Task};

use super::{BatchOperationResult, BatchTaskResult};

/// Collect unique task IDs from a list of tasks.
pub fn collect_task_ids(tasks: &[&Task]) -> Vec<String> {
    tasks.iter().map(|task| task.id.clone()).collect()
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
    queue: &QueueFile,
    task_ids: &[String],
) -> anyhow::Result<()> {
    use anyhow::bail;

    for task_id in task_ids {
        let needle = task_id.trim();
        if needle.is_empty() {
            bail!("Empty task ID provided");
        }
        if !queue.tasks.iter().any(|task| task.id.trim() == needle) {
            bail!(
                "{}",
                crate::error_messages::task_not_found_batch_failure(needle)
            );
        }
    }
    Ok(())
}

/// Collector for batch operation results with standardized error handling.
///
/// Responsibilities:
/// - Track success/failure counts.
/// - Collect individual task results.
/// - Handle continue-on-error semantics.
///
/// Does not handle:
/// - Task ID deduplication (use `preprocess_batch_ids`).
/// - Actual operation execution (caller responsibility).
pub(crate) struct BatchResultCollector {
    total: usize,
    results: Vec<BatchTaskResult>,
    succeeded: usize,
    failed: usize,
    continue_on_error: bool,
    op_name: &'static str,
}

impl BatchResultCollector {
    /// Create a new collector for a batch operation.
    pub fn new(total: usize, continue_on_error: bool, op_name: &'static str) -> Self {
        Self {
            total,
            results: Vec::with_capacity(total),
            succeeded: 0,
            failed: 0,
            continue_on_error,
            op_name,
        }
    }

    /// Record a successful operation on a task.
    pub fn record_success(&mut self, task_id: String, created_task_ids: Vec<String>) {
        self.results.push(BatchTaskResult {
            task_id,
            success: true,
            error: None,
            created_task_ids,
        });
        self.succeeded += 1;
    }

    /// Record a failed operation on a task.
    ///
    /// Returns an error if not in continue-on-error mode, allowing caller to propagate.
    pub fn record_failure(&mut self, task_id: String, error: String) -> anyhow::Result<()> {
        self.results.push(BatchTaskResult {
            task_id: task_id.clone(),
            success: false,
            error: Some(error.clone()),
            created_task_ids: Vec::new(),
        });
        self.failed += 1;

        if !self.continue_on_error {
            anyhow::bail!(
                "Batch {} failed at task {}: {}. Use --continue-on-error to process remaining tasks.",
                self.op_name,
                task_id,
                error
            );
        }
        Ok(())
    }

    /// Consume the collector and return the final result.
    pub fn finish(self) -> BatchOperationResult {
        BatchOperationResult {
            total: self.total,
            succeeded: self.succeeded,
            failed: self.failed,
            results: self.results,
        }
    }
}

/// Preprocess task IDs for batch operations.
///
/// Deduplicates task IDs and validates the list is not empty.
pub(crate) fn preprocess_batch_ids(
    task_ids: &[String],
    op_name: &str,
) -> anyhow::Result<Vec<String>> {
    let unique_ids = deduplicate_task_ids(task_ids);
    if unique_ids.is_empty() {
        anyhow::bail!("No task IDs provided for batch {}", op_name);
    }
    Ok(unique_ids)
}
