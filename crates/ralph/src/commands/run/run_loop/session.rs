//! Session recovery helpers for the sequential run loop.
//!
//! Purpose:
//! - Session recovery helpers for the sequential run loop.
//!
//! Responsibilities:
//! - Resolve whether the loop should resume a prior task session.
//! - Centralize operator-facing resume/fresh/refusal decisions before task selection.
//!
//! Not handled here:
//! - Queue waiting or task execution.
//! - Session progress persistence after task execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Session timeout uses configured hours (defaulting to the shared constant).
//! - Prompt-required non-interactive cases refuse instead of guessing.

use crate::commands::run::{emit_blocked_state_changed, emit_resume_decision};
use crate::config;
use crate::session::{self, ResumeBehavior, ResumeDecisionMode, ResumeStatus};
use anyhow::{Result, bail};

use super::RunLoopOptions;

pub(super) struct ResumeState {
    pub(super) resume_task_id: Option<String>,
    pub(super) completed_count: u32,
}

pub(super) fn resolve_resume_state(
    resolved: &config::Resolved,
    opts: &RunLoopOptions,
) -> Result<ResumeState> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let queue_file = crate::queue::load_queue(&resolved.queue_path)?;
    let resolution = session::resolve_run_session_decision(
        &cache_dir,
        &queue_file,
        session::RunSessionDecisionOptions {
            timeout_hours: resolved.config.agent.session_timeout_hours,
            behavior: if opts.auto_resume {
                ResumeBehavior::AutoResume
            } else {
                ResumeBehavior::Prompt
            },
            non_interactive: opts.non_interactive,
            explicit_task_id: None,
            announce_missing_session: opts.auto_resume,
            mode: ResumeDecisionMode::Execute,
        },
    )?;

    if let Some(decision) = resolution.decision.as_ref() {
        emit_resume_decision(decision, opts.run_event_handler.as_ref());
        if let Some(blocking_state) = decision.blocking_state() {
            emit_blocked_state_changed(&blocking_state, opts.run_event_handler.as_ref());
        }
        if matches!(decision.status, ResumeStatus::RefusingToResume) {
            bail!("{}", decision.message);
        }
    }

    let completed_count = if resolution.resume_task_id.is_some() {
        resolution.completed_count
    } else {
        opts.starting_completed
    };

    Ok(ResumeState {
        resume_task_id: resolution.resume_task_id,
        completed_count,
    })
}
