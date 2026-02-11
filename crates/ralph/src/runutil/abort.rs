//! Run abort classification utilities.
//!
//! Responsibilities:
//! - Provide an error type (`RunAbort`) used to short-circuit the run loop.
//! - Provide a classifier (`abort_reason`) that detects abort causes in an anyhow chain.
//!
//! Not handled here:
//! - Runner execution, revert prompting, or IO.
//!
//! Invariants/assumptions:
//! - `RunAbort` is always wrapped inside an `anyhow::Error` when propagated.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunAbortReason {
    Interrupted,
    UserRevert,
}

#[derive(Debug)]
pub(crate) struct RunAbort {
    reason: RunAbortReason,
    message: String,
}

impl RunAbort {
    pub(crate) fn new(reason: RunAbortReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }

    pub(crate) fn reason(&self) -> RunAbortReason {
        self.reason
    }
}

impl fmt::Display for RunAbort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RunAbort {}

pub(crate) fn abort_reason(err: &anyhow::Error) -> Option<RunAbortReason> {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<RunAbort>().map(RunAbort::reason))
}

/// Check if an error is a dirty repository error.
///
/// This is used to detect non-retryable errors when the repository has
/// uncommitted changes that prevent the run loop from proceeding.
pub(crate) fn is_dirty_repo_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<crate::git::GitError>()
            .is_some_and(|e| matches!(e, crate::git::GitError::DirtyRepo { .. }))
    })
}

/// Check if an error is a queue validation error.
///
/// Queue validation errors cannot self-heal through retries because they
/// indicate structural problems in the queue file that require user intervention.
/// This is used to detect non-retryable errors to prevent the 50-failure abort loop.
pub(crate) fn is_queue_validation_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        let msg = cause.to_string();
        // Match common queue validation error patterns from validation.rs
        msg.contains("relationship")
            || msg.contains("Duplicate task ID")
            || msg.contains("Circular blocking")
            || msg.starts_with("Self-")
            || msg.starts_with("Invalid ")
            || msg.starts_with("Missing ")
            || msg.starts_with("Unsupported queue.json")
            || msg.starts_with("Empty id_prefix")
            || msg.starts_with("Invalid id_width")
    })
}
