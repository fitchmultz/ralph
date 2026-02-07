//! Run-one orchestration entrypoints.
//!
//! Responsibilities:
//! - Provide public entrypoints (`run_one*`) used by the CLI and interactive flows.
//! - Own lock-acquisition policy for run-one execution.
//!
//! Not handled here:
//! - Run loop orchestration (see `run_loop`).
//! - Queue selection helper primitives (see `selection`).
//! - Phase execution details (see `phases`).
//!
//! Invariants/assumptions:
//! - `run_one_with_id_locked` is called only when the queue lock is already held by the caller.
//! - Parallel-worker mode must not mutate queue/done files.

use crate::agent::AgentOverrides;
use crate::config;
use crate::runner;
use crate::runutil;
use anyhow::Result;

mod orchestration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueLockMode {
    Acquire,
    Held,
    /// Acquire the queue lock but allow creating upstream branches (used by parallel workers).
    /// This combines the safety of lock acquisition with the push policy of Skip mode.
    AcquireAllowUpstream,
}

/// Outcome of a single task run.
#[derive(Debug)]
pub enum RunOutcome {
    /// No Todo (and no Draft if include_draft is false).
    NoCandidates,
    /// Candidates exist, but none are currently runnable (deps/schedule/status flags).
    Blocked {
        summary: crate::queue::operations::QueueRunnabilitySummary,
    },
    Ran {
        task_id: String,
    },
}

/// Run a specific task by ID.
pub fn run_one_with_id(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        Some(task_id),
        None,
        output_handler,
        revert_prompt,
    )
    .map(|_| ())
}

/// Run a specific task as a parallel worker (acquires queue lock, allows upstream creation).
pub fn run_one_parallel_worker(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
) -> Result<()> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::AcquireAllowUpstream,
        Some(task_id),
        None,
        None,
        None,
    )
    .map(|_| ())
}

/// Run a specific task when the queue lock is already held by the caller.
pub fn run_one_with_id_locked(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Held,
        Some(task_id),
        None,
        output_handler,
        revert_prompt,
    )
    .map(|_| ())
}

/// Run the first available todo task.
pub fn run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    resume_task_id: Option<&str>,
) -> Result<RunOutcome> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        None,
        resume_task_id,
        None,
        None,
    )
}
