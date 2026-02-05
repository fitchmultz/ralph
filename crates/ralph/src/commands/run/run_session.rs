//! Session management for run orchestration.
//!
//! Responsibilities:
//! - Create session state for crash recovery.
//! - Validate resumed tasks before continuing execution.
//!
//! Not handled here:
//! - Session persistence (handled by `crate::session`).
//! - Session loading and clearing (handled by `crate::session`).
//!
//! Invariants/assumptions:
//! - Session IDs are unique and include timestamp + task_id.
//! - Phase settings are captured at session creation time.

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::{PhaseSettingsSnapshot, SessionState, TaskStatus};
use crate::runner::PhaseSettingsMatrix;
use crate::session;
use crate::timeutil;
use anyhow::Result;

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
    let runner = agent_overrides
        .runner
        .clone()
        .or(resolved.config.agent.runner.clone())
        .unwrap_or(crate::contracts::Runner::Claude);

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
        .unwrap_or_else(|| "sonnet".to_string());

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
/// Returns Ok(()) if valid, Err with message if not.
/// On validation failure, clears the session file.
pub(crate) fn validate_resumed_task(
    queue_file: &crate::contracts::QueueFile,
    task_id: &str,
    repo_root: &std::path::Path,
) -> Result<()> {
    let task = queue_file
        .tasks
        .iter()
        .find(|t| t.id.trim() == task_id)
        .ok_or_else(|| {
            let cache_dir = repo_root.join(".ralph/cache");
            if let Err(e) = session::clear_session(&cache_dir) {
                log::debug!("Failed to clear invalid session: {}", e);
            }
            anyhow::anyhow!("Task {} no longer exists in queue", task_id)
        })?;

    if task.status != TaskStatus::Doing {
        let cache_dir = repo_root.join(".ralph/cache");
        if let Err(e) = session::clear_session(&cache_dir) {
            log::debug!("Failed to clear invalid session: {}", e);
        }
        return Err(anyhow::anyhow!(
            "Task {} is not in Doing status (current: {})",
            task_id,
            task.status
        ));
    }

    Ok(())
}
