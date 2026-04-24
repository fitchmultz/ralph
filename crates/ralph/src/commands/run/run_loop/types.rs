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

use crate::agent::AgentOverrides;
use crate::commands::run::RunEventHandler;

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

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct RunLoopStats {
    pub(super) tasks_attempted: usize,
    pub(super) tasks_succeeded: usize,
    pub(super) tasks_failed: usize,
    pub(super) consecutive_failures: u32,
}
