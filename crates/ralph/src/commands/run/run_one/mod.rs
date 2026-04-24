//! Run-one orchestration entrypoints.
//!
//! Purpose:
//! - Run-one orchestration entrypoints.
//!
//! Responsibilities:
//! - Provide public entrypoints (`run_one*`) used by the CLI and interactive flows.
//! - Own lock-acquisition policy for run-one execution.
//! - Define resume-resolution options for direct and loop-driven run-one calls.
//!
//! Not handled here:
//! - Run loop orchestration (see `run_loop`).
//! - Queue selection helper primitives (see `selection`).
//! - Phase execution details (see `phases`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `run_one_with_id_locked` is called only when the queue lock is already held by the caller.
//! - Parallel-worker mode resolves queue/done from worker workspace paths.

use crate::agent::AgentOverrides;
use crate::commands::run::RunEventHandler;
use crate::config;
use crate::runner;
use crate::runutil;
use anyhow::Result;

mod completion;
mod context;
mod execution_setup;
mod orchestration;
mod phase_execution;
mod selection;
mod webhooks;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueLockMode {
    Acquire,
    Held,
    /// Acquire the queue lock but allow creating upstream branches (used by parallel workers).
    /// This combines the safety of lock acquisition with the push policy of Skip mode.
    AcquireAllowUpstream,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RunOneResumeOptions {
    pub auto_resume: bool,
    pub non_interactive: bool,
    pub resume_task_id: Option<String>,
    pub detect_session: bool,
}

impl RunOneResumeOptions {
    pub fn detect(auto_resume: bool, non_interactive: bool) -> Self {
        Self {
            auto_resume,
            non_interactive,
            resume_task_id: None,
            detect_session: true,
        }
    }

    pub fn resolved(resume_task_id: Option<String>) -> Self {
        Self {
            auto_resume: false,
            non_interactive: false,
            resume_task_id,
            detect_session: false,
        }
    }

    pub fn disabled() -> Self {
        Self::default()
    }
}

/// Outcome of a single task run.
#[derive(Debug)]
pub enum RunOutcome {
    /// No Todo (and no Draft if include_draft is false).
    NoCandidates,
    /// Candidates exist, but none are currently runnable (deps/schedule/status flags).
    Blocked {
        summary: Box<crate::queue::operations::QueueRunnabilitySummary>,
        state: Box<crate::contracts::BlockingState>,
    },
    Ran {
        task_id: String,
    },
}

/// Run a specific task by ID.
#[allow(clippy::too_many_arguments)]
pub fn run_one_with_id(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    resume_options: RunOneResumeOptions,
    output_handler: Option<runner::OutputHandler>,
    run_event_handler: Option<RunEventHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        Some(task_id),
        resume_options,
        output_handler,
        run_event_handler,
        revert_prompt,
        None,
    )
    .map(|_| ())
}

/// Run a specific task as a parallel worker (acquires queue lock, allows upstream creation).
pub fn run_one_parallel_worker(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    target_branch: &str,
) -> Result<()> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::AcquireAllowUpstream,
        Some(task_id),
        RunOneResumeOptions::disabled(),
        None,
        None,
        None,
        Some(target_branch),
    )
    .map(|_| ())
}

/// Run a specific task when the queue lock is already held by the caller.
#[allow(clippy::too_many_arguments)]
pub fn run_one_with_id_locked(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    resume_options: RunOneResumeOptions,
    output_handler: Option<runner::OutputHandler>,
    run_event_handler: Option<RunEventHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Held,
        Some(task_id),
        resume_options,
        output_handler,
        run_event_handler,
        revert_prompt,
        None,
    )
    .map(|_| ())
}

/// Run the first available todo task.
pub fn run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    resume_options: RunOneResumeOptions,
) -> Result<RunOutcome> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        None,
        resume_options,
        None,
        None,
        None,
        None,
    )
}

/// Run the first available task with streaming handlers.
pub fn run_one_with_handlers(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    resume_options: RunOneResumeOptions,
    output_handler: Option<runner::OutputHandler>,
    run_event_handler: Option<RunEventHandler>,
) -> Result<RunOutcome> {
    orchestration::run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        None,
        resume_options,
        output_handler,
        run_event_handler,
        None,
        None,
    )
}
