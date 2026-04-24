//! Task queue task-level operations.
//!
//! Purpose:
//! - Task queue task-level operations.
//!
//! Responsibilities:
//! - Mutate or query tasks within queue files (complete tasks, set statuses/fields, find tasks, delete tasks, sort by priority).
//! - Provide typed domain errors for queue query operations to enable stable test assertions.
//!
//! Non-scope:
//! - Persisting queue data or managing locks (load/save/locks/repair live in `crate::queue`).
//! - Task-level validation beyond runnability checks (see `validate` module for schema-level validation).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue operations are called with fully loaded `QueueFile` values.
//! - Queue query errors are returned as `anyhow::Result` but wrap typed `QueueQueryError` for downcasting in tests.
//! - Message text in `QueueQueryError` variants must match user-facing expectations (single source of truth).

mod archive;
mod batch;
mod edit;
mod fields;
mod followups;
mod mutation;
mod query;
mod runnability;
mod status;
mod transaction;
mod validate;

pub use archive::*;
pub use batch::*;
pub use edit::*;
pub use fields::*;
pub use followups::*;
pub use mutation::*;
pub use query::*;
pub use runnability::*;
pub use status::*;
pub use transaction::*;

#[cfg(test)]
#[path = "operations/tests/mod.rs"]
mod tests;

use crate::contracts::TaskStatus;
use crate::error_messages::task_not_found_with_operation;

#[derive(Debug, thiserror::Error)]
pub enum QueueQueryError {
    #[error(
        "Queue query failed (operation={operation}): missing target_task_id. Example: --target RQ-0001."
    )]
    MissingTargetTaskId { operation: String },

    #[error("{}", task_not_found_with_operation(operation, task_id))]
    TargetTaskNotFound { operation: String, task_id: String },

    #[error(
        "Queue query failed (operation={operation}): target task {task_id} is not runnable (status: {status}). Choose a todo/doing task."
    )]
    TargetTaskNotRunnable {
        operation: String,
        task_id: String,
        status: TaskStatus,
    },

    #[error(
        "Queue query failed (operation={operation}): target task {task_id} is in draft status. Use --include-draft to run draft tasks."
    )]
    TargetTaskDraftExcluded { operation: String, task_id: String },

    #[error(
        "Queue query failed (operation={operation}): target task {task_id} is blocked by unmet dependencies. Resolve dependencies before running."
    )]
    TargetTaskBlockedByUnmetDependencies { operation: String, task_id: String },

    #[error(
        "Queue query failed (operation={operation}): target task {task_id} is scheduled for the future ({scheduled_start}). Wait until the scheduled time."
    )]
    TargetTaskScheduledForFuture {
        operation: String,
        task_id: String,
        scheduled_start: String,
    },
}
