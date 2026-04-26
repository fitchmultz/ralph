//! Shared types for sequential run-loop orchestration.
//!
//! Purpose:
//! - Shared types for sequential run-loop orchestration.
//!
//! Responsibilities:
//! - Define configuration and mutable bookkeeping for the sequential run loop.
//!
//! Not handled here:
//! - The loop state machine itself.
//! - Wait or session recovery behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `max_tasks == 0` means unbounded execution.

use anyhow::{Result, bail};

use crate::agent::AgentOverrides;
use crate::commands::run::RunEventHandler;
use crate::contracts::{BlockingReason, BlockingState};
use crate::queue::operations::QueueRunnabilitySummary;

pub struct RunLoopOptions {
    pub max_tasks: u32,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
    pub auto_resume: bool,
    pub starting_completed: u32,
    pub non_interactive: bool,
    pub parallel_workers: Option<u8>,
    pub wait_when_blocked: bool,
    pub wait_poll_ms: u64,
    pub wait_timeout_seconds: u64,
    pub notify_when_unblocked: bool,
    pub wait_when_empty: bool,
    pub empty_poll_ms: u64,
    pub run_event_handler: Option<RunEventHandler>,
}

#[derive(Debug, Clone)]
pub enum RunLoopOutcome {
    Completed,
    NoCandidates {
        blocking: Box<BlockingState>,
    },
    Blocked {
        summary: Box<QueueRunnabilitySummary>,
        blocking: Box<BlockingState>,
    },
    Stalled {
        blocking: Box<BlockingState>,
    },
    Stopped {
        blocking: Option<Box<BlockingState>>,
    },
}

impl RunLoopOutcome {
    pub fn into_non_machine_result(self) -> Result<Self> {
        if let Self::Stalled { blocking } = &self
            && matches!(blocking.reason, BlockingReason::RunnerRecovery { .. })
        {
            bail!("{}", blocking.message);
        }
        Ok(self)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct RunLoopStats {
    pub(super) tasks_attempted: usize,
    pub(super) tasks_succeeded: usize,
    pub(super) tasks_failed: usize,
    pub(super) consecutive_failures: u32,
}

#[cfg(test)]
mod tests {
    use super::RunLoopOutcome;
    use crate::contracts::BlockingState;

    #[test]
    fn non_machine_result_rejects_runner_recovery_stall() {
        let outcome = RunLoopOutcome::Stalled {
            blocking: Box::new(BlockingState::runner_recovery(
                "run_session",
                "session_timed_out_requires_confirmation",
                Some("RQ-1234".to_string()),
                "Resume: refusing to continue timed-out session RQ-1234 without explicit confirmation.",
                "The saved session is too old.",
            )),
        };
        let err = outcome
            .into_non_machine_result()
            .expect_err("runner recovery should stay terminal outside machine surfaces");
        assert!(
            err.to_string()
                .contains("Resume: refusing to continue timed-out session RQ-1234")
        );
    }

    #[test]
    fn non_machine_result_keeps_idle_outcome_successful() {
        let outcome = RunLoopOutcome::NoCandidates {
            blocking: Box::new(BlockingState::idle(false)),
        };
        assert!(matches!(
            outcome
                .into_non_machine_result()
                .expect("idle is not an error"),
            RunLoopOutcome::NoCandidates { .. }
        ));
    }
}
