//! Session management for run orchestration.
//!
//! Purpose:
//! - Session management for run orchestration.
//!
//! Responsibilities:
//! - Create session state for crash recovery.
//! - Validate resumed tasks before continuing execution.
//! - Convert invalid resume targets into explicit fresh-start decisions.
//!
//! Not handled here:
//! - Session loading and clearing orchestration beyond invalid-target cleanup.
//! - Session prompt handling.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Session IDs are unique and include timestamp + task_id.
//! - Phase settings are captured at session creation time.

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::{Config, PhaseSettingsSnapshot, SessionState, TaskStatus};
use crate::runner::PhaseSettingsMatrix;
use crate::session;
use crate::timeutil;
use anyhow::Result;

pub(crate) enum ResumeTaskValidation {
    Resumable,
    FreshStart(crate::session::ResumeDecision),
}

/// Create a session state for the current task.
pub(crate) fn create_session_for_task(
    task_id: &str,
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    iterations_planned: u8,
    phase_matrix: Option<&PhaseSettingsMatrix>,
) -> SessionState {
    let now = timeutil::now_utc_rfc3339_or_fallback();
    let git_commit = session::get_git_head_commit(&resolved.repo_root);

    // Resolve runner from overrides or config
    let default_config = Config::default();

    let runner = agent_overrides
        .runner
        .clone()
        .or(resolved.config.agent.runner.clone())
        .or(default_config.agent.runner.clone())
        .unwrap_or_default();

    // Resolve model string from overrides or config
    let model = agent_overrides
        .model
        .as_ref()
        .map(|m| m.as_str().to_string())
        .or_else(|| {
            resolved
                .config
                .agent
                .model
                .as_ref()
                .map(|m| m.as_str().to_string())
        })
        .or_else(|| {
            default_config
                .agent
                .model
                .as_ref()
                .map(|m| m.as_str().to_string())
        })
        .unwrap_or_else(|| "gpt-5.4".to_string());

    // Generate a simple session ID using timestamp and task ID
    let session_id = format!("{}-{}", now.replace([':', '.', '-'], ""), task_id);

    // Build phase settings snapshot from resolved matrix
    let phase_settings = phase_matrix.map(|matrix| {
        (
            PhaseSettingsSnapshot {
                runner: matrix.phase1.runner.clone(),
                model: matrix.phase1.model.as_str().to_string(),
                reasoning_effort: matrix.phase1.reasoning_effort,
            },
            PhaseSettingsSnapshot {
                runner: matrix.phase2.runner.clone(),
                model: matrix.phase2.model.as_str().to_string(),
                reasoning_effort: matrix.phase2.reasoning_effort,
            },
            PhaseSettingsSnapshot {
                runner: matrix.phase3.runner.clone(),
                model: matrix.phase3.model.as_str().to_string(),
                reasoning_effort: matrix.phase3.reasoning_effort,
            },
        )
    });

    SessionState::new(
        session_id,
        task_id.to_string(),
        now,
        iterations_planned,
        runner,
        model,
        0, // max_tasks - not tracked at this level
        git_commit,
        phase_settings,
    )
}

/// Validate that a resumed task exists and is in a runnable state.
/// On validation failure (task missing or terminal), clears the session file and returns
/// an explicit fresh-start decision.
pub(crate) fn validate_resumed_task(
    queue_file: &crate::contracts::QueueFile,
    task_id: &str,
    repo_root: &std::path::Path,
) -> Result<ResumeTaskValidation> {
    let cache_dir = repo_root.join(".ralph/cache");
    let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == task_id) else {
        if let Err(err) = session::clear_session(&cache_dir) {
            log::debug!("Failed to clear invalid session: {}", err);
        }
        return Ok(ResumeTaskValidation::FreshStart(
            crate::session::ResumeDecision {
                status: crate::session::ResumeStatus::FallingBackToFreshInvocation,
                scope: crate::session::ResumeScope::RunSession,
                reason: crate::session::ResumeReason::ResumeTargetMissing,
                task_id: Some(task_id.to_string()),
                message: format!(
                    "Resume: starting fresh because interrupted task {} no longer exists.",
                    task_id
                ),
                detail: crate::error_messages::task_no_longer_exists(task_id),
            },
        ));
    };

    // Only invalidate the session for terminal states (Done, Rejected).
    // Todo and Doing are both valid states for resumption:
    // - Doing: task was interrupted mid-execution (classic crash recovery)
    // - Todo: task was marked doing but failed before any work was done
    if task.status == TaskStatus::Done || task.status == TaskStatus::Rejected {
        if let Err(err) = session::clear_session(&cache_dir) {
            log::debug!("Failed to clear invalid session: {}", err);
        }
        return Ok(ResumeTaskValidation::FreshStart(
            crate::session::ResumeDecision {
                status: crate::session::ResumeStatus::FallingBackToFreshInvocation,
                scope: crate::session::ResumeScope::RunSession,
                reason: crate::session::ResumeReason::ResumeTargetTerminal,
                task_id: Some(task_id.to_string()),
                message: format!(
                    "Resume: starting fresh because task {} is already {}.",
                    task_id, task.status
                ),
                detail: format!(
                    "Interrupted session cannot continue because the task is already {}.",
                    task.status
                ),
            },
        ));
    }

    Ok(ResumeTaskValidation::Resumable)
}
