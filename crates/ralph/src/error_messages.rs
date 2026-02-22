//! Canonical error message constructors.
//!
//! Responsibilities:
//! - Provide helper functions for all "task not found" error scenarios
//! - Ensure consistent formatting and actionable hints
//!
//! Does not handle:
//! - Error types (use anyhow/thiserror in consuming modules)
//! - I18N (messages are English-only)
//!
//! Invariants/assumptions:
//! - All task ID parameters are non-empty trimmed strings
//! - Messages include actionable hints where appropriate

/// Task not found in the active queue only.
pub fn task_not_found_in_queue(task_id: &str) -> String {
    format!("Task '{task_id}' not found in active queue.")
}

/// Task not found in either queue or done archive.
pub fn task_not_found_in_queue_or_done(task_id: &str) -> String {
    format!("Task '{task_id}' not found in queue or done archive.")
}

/// Task not found with hint to use --include-done.
pub fn task_not_found_with_include_done_hint(task_id: &str) -> String {
    format!(
        "Task '{task_id}' not found in active queue. \
         Use --include-done to search the done archive."
    )
}

/// Root task not found (for tree/graph commands).
pub fn root_task_not_found(task_id: &str, include_done: bool) -> String {
    if include_done {
        format!("Root task '{task_id}' not found in queue or done archive.")
    } else {
        format!(
            "Root task '{task_id}' not found in active queue. \
             Use --include-done to search the done archive."
        )
    }
}

/// Source task not found (for clone/split operations).
pub fn source_task_not_found(task_id: &str, search_done: bool) -> String {
    if search_done {
        format!("Source task '{task_id}' not found in queue or done archive.")
    } else {
        format!("Source task '{task_id}' not found in active queue.")
    }
}

/// Task not found for batch operations (recorded as failure).
pub fn task_not_found_batch_failure(task_id: &str) -> String {
    format!("Task not found: {task_id}")
}

/// Task not found with operation context (for QueueQueryError).
pub fn task_not_found_with_operation(operation: &str, task_id: &str) -> String {
    format!(
        "Queue query failed (operation={operation}): \
         target task not found: {task_id}. \
         Ensure it exists in .ralph/queue.jsonc."
    )
}

/// Task not found in done archive specifically.
pub fn task_not_found_in_done_archive(task_id: &str, context: &str) -> String {
    format!("Task '{task_id}' not found in done archive for {context}.")
}

/// Task not found for edit operations with file context.
pub fn task_not_found_for_edit(operation: &str, task_id: &str) -> String {
    format!(
        "Queue {operation} failed (task_id={task_id}): \
         task not found in .ralph/queue.jsonc."
    )
}

/// Generic task not found (for simple cases).
pub fn task_not_found(task_id: &str) -> String {
    format!("Task not found: {task_id}")
}

/// Task no longer exists (for session recovery scenarios).
/// Used when a task was deleted during execution.
pub fn task_no_longer_exists(task_id: &str) -> String {
    format!("Task '{task_id}' no longer exists in queue (may have been deleted or archived).")
}
