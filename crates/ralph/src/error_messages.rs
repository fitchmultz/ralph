//! Canonical error message constructors.
//!
//! Purpose:
//! - Canonical error message constructors.
//!
//! Responsibilities:
//! - Provide helper functions for all "task not found" error scenarios
//! - Ensure consistent formatting and actionable hints
//!
//! Non-scope:
//! - Error types (use anyhow/thiserror in consuming modules)
//! - I18N (messages are English-only)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All task ID parameters are non-empty trimmed strings
//! - Messages include actionable hints where appropriate

fn queue_search_scope(include_done: bool) -> &'static str {
    if include_done {
        "queue or done archive"
    } else {
        "active queue"
    }
}

fn task_not_found_in_scope(
    subject: &str,
    task_id: &str,
    include_done: bool,
    include_done_hint: bool,
) -> String {
    let mut message = format!(
        "{subject} '{task_id}' not found in {}.",
        queue_search_scope(include_done)
    );
    if !include_done && include_done_hint {
        message.push_str(" Use --include-done to search the done archive.");
    }
    message
}

fn task_not_found_short(task_id: &str) -> String {
    format!("Task not found: {task_id}")
}

/// Task not found in the active queue only.
pub fn task_not_found_in_queue(task_id: &str) -> String {
    task_not_found_in_scope("Task", task_id, false, false)
}

/// Task not found in either queue or done archive.
pub fn task_not_found_in_queue_or_done(task_id: &str) -> String {
    task_not_found_in_scope("Task", task_id, true, false)
}

/// Task not found with hint to use --include-done.
pub fn task_not_found_with_include_done_hint(task_id: &str) -> String {
    task_not_found_in_scope("Task", task_id, false, true)
}

/// Root task not found (for tree/graph commands).
pub fn root_task_not_found(task_id: &str, include_done: bool) -> String {
    task_not_found_in_scope("Root task", task_id, include_done, !include_done)
}

/// Source task not found (for clone/split operations).
pub fn source_task_not_found(task_id: &str, search_done: bool) -> String {
    task_not_found_in_scope("Source task", task_id, search_done, false)
}

/// Task not found for batch operations (recorded as failure).
pub fn task_not_found_batch_failure(task_id: &str) -> String {
    task_not_found_short(task_id)
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
    task_not_found_short(task_id)
}

/// Task no longer exists (for session recovery scenarios).
/// Used when a task was deleted during execution.
pub fn task_no_longer_exists(task_id: &str) -> String {
    format!("Task '{task_id}' no longer exists in queue (may have been deleted or archived).")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_not_found_in_active_queue_matches_contract() {
        assert_eq!(
            task_not_found_in_queue("RQ-100"),
            "Task 'RQ-100' not found in active queue."
        );
    }

    #[test]
    fn task_not_found_in_queue_or_done_matches_contract() {
        assert_eq!(
            task_not_found_in_queue_or_done("RQ-100"),
            "Task 'RQ-100' not found in queue or done archive."
        );
    }

    #[test]
    fn task_not_found_with_include_done_hint_matches_contract() {
        assert_eq!(
            task_not_found_with_include_done_hint("RQ-100"),
            "Task 'RQ-100' not found in active queue. Use --include-done to search the done archive."
        );
    }

    #[test]
    fn root_task_not_found_without_done_search_includes_hint() {
        assert_eq!(
            root_task_not_found("RQ-100", false),
            "Root task 'RQ-100' not found in active queue. Use --include-done to search the done archive."
        );
    }

    #[test]
    fn root_task_not_found_with_done_search_matches_contract() {
        assert_eq!(
            root_task_not_found("RQ-100", true),
            "Root task 'RQ-100' not found in queue or done archive."
        );
    }

    #[test]
    fn source_task_not_found_without_done_search_matches_contract() {
        assert_eq!(
            source_task_not_found("RQ-100", false),
            "Source task 'RQ-100' not found in active queue."
        );
    }

    #[test]
    fn source_task_not_found_with_done_search_matches_contract() {
        assert_eq!(
            source_task_not_found("RQ-100", true),
            "Source task 'RQ-100' not found in queue or done archive."
        );
    }

    #[test]
    fn generic_task_not_found_matches_contract() {
        assert_eq!(task_not_found("RQ-100"), "Task not found: RQ-100");
    }

    #[test]
    fn batch_task_not_found_matches_contract() {
        assert_eq!(
            task_not_found_batch_failure("RQ-100"),
            "Task not found: RQ-100"
        );
    }

    #[test]
    fn task_not_found_for_edit_matches_contract() {
        assert_eq!(
            task_not_found_for_edit("status", "RQ-100"),
            "Queue status failed (task_id=RQ-100): task not found in .ralph/queue.jsonc."
        );
    }

    #[test]
    fn task_not_found_with_operation_matches_contract() {
        assert_eq!(
            task_not_found_with_operation("edit", "RQ-100"),
            "Queue query failed (operation=edit): target task not found: RQ-100. Ensure it exists in .ralph/queue.jsonc."
        );
    }

    #[test]
    fn task_not_found_in_done_archive_matches_contract() {
        assert_eq!(
            task_not_found_in_done_archive("RQ-100", "restore"),
            "Task 'RQ-100' not found in done archive for restore."
        );
    }

    #[test]
    fn task_no_longer_exists_matches_contract() {
        assert_eq!(
            task_no_longer_exists("RQ-100"),
            "Task 'RQ-100' no longer exists in queue (may have been deleted or archived)."
        );
    }
}
