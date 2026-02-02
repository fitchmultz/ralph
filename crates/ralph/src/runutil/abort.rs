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
