//! Run-one context preparation.
//!
//! Responsibilities:
//! - Prepare the context for run-one execution (lock, queue, config).
//! - Handle Ctrl+C state initialization and pre-run interrupt detection.
//! - Load and validate queues.
//! - Resolve git and prompt policy configuration.
//!
//! Not handled here:
//! - Task execution setup (see execution_setup.rs).
//! - Phase execution (see phase_execution.rs).
//!
//! Invariants/assumptions:
//! - Callers pass the correct `QueueLockMode` for their context.

use super::QueueLockMode;
use super::orchestration::RunOneContext;
use crate::agent::AgentOverrides;
use crate::commands::run::{phases::PostRunMode, supervision::PushPolicy};
use crate::config;
use crate::contracts::GitRevertMode;
use crate::{promptflow, queue};
use anyhow::Result;

/// Prepare the context for run-one execution.
///
/// Handles Ctrl+C state initialization and pre-run interrupt detection,
/// lock acquisition, queue loading/validation, and configuration resolution.
pub(crate) fn prepare_run_one_context(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    lock_mode: QueueLockMode,
    parallel_target_branch: Option<&str>,
) -> Result<RunOneContext> {
    // Handle Ctrl+C state initialization and pre-run interrupt detection.
    let ctrlc = crate::runner::ctrlc_state()
        .map_err(|e| anyhow::anyhow!("Ctrl-C handler initialization failed: {}", e))?;

    if ctrlc.interrupted.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(crate::runutil::RunAbort::new(
            crate::runutil::RunAbortReason::Interrupted,
            "Ctrl+C was pressed before task execution started",
        )
        .into());
    }

    ctrlc
        .interrupted
        .store(false, std::sync::atomic::Ordering::SeqCst);

    let _queue_lock = match lock_mode {
        QueueLockMode::Acquire | QueueLockMode::AcquireAllowUpstream => Some(
            queue::acquire_queue_lock(&resolved.repo_root, "run one", force)?,
        ),
        QueueLockMode::Held => None,
    };

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = queue::validate_queue_set(
        &queue_file,
        Some(&done),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

    let repoprompt_flags =
        crate::agent::resolve_repoprompt_flags_from_overrides(agent_overrides, resolved);

    let git_revert_mode = agent_overrides
        .git_revert_mode
        .or(resolved.config.agent.git_revert_mode)
        .unwrap_or(GitRevertMode::Ask);

    let git_commit_push_enabled = agent_overrides
        .git_commit_push_enabled
        .or(resolved.config.agent.git_commit_push_enabled)
        .unwrap_or(true);

    let push_policy = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => PushPolicy::AllowCreateUpstream,
        QueueLockMode::Acquire | QueueLockMode::Held => PushPolicy::RequireUpstream,
    };

    let post_run_mode = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => PostRunMode::ParallelWorker,
        QueueLockMode::Acquire | QueueLockMode::Held => PostRunMode::Normal,
    };

    let parallel_target_branch = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => {
            let branch = parallel_target_branch
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "parallel worker requires explicit target branch (--parallel-target-branch)"
                    )
                })?;
            Some(branch.to_string())
        }
        QueueLockMode::Acquire | QueueLockMode::Held => None,
    };

    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: repoprompt_flags.plan_required,
        repoprompt_tool_injection: repoprompt_flags.tool_injection,
    };

    Ok(RunOneContext {
        queue_file,
        done,
        git_revert_mode,
        git_commit_push_enabled,
        push_policy,
        post_run_mode,
        parallel_target_branch,
        policy,
    })
}
