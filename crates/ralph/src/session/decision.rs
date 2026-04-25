//! Operator-facing resume decision model and session-resolution helpers.
//!
//! Purpose:
//! - Operator-facing resume decision model and session-resolution helpers.
//!
//! Responsibilities:
//! - Convert low-level session validation into explicit resume/fresh/refusal decisions.
//! - Preserve machine-readable decision state for CLI/app surfaces.
//! - Apply session-cache mutations only when the caller is executing a real run.
//!
//! Not handled here:
//! - Session persistence IO details.
//! - Queue/task execution.
//! - Continue-session runner resumption.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Timed-out sessions always require explicit confirmation.
//! - Non-interactive prompt-required cases refuse instead of guessing.
//! - Preview callers must not mutate session cache state.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::contracts::{BlockingState, QueueFile};

use super::{
    SessionValidationResult, check_session, clear_session, prompt_session_recovery,
    prompt_session_recovery_timeout,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResumeStatus {
    ResumingSameSession,
    FallingBackToFreshInvocation,
    RefusingToResume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResumeScope {
    RunSession,
    ContinueSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResumeReason {
    NoSession,
    SessionValid,
    SessionTimedOutConfirmed,
    SessionStale,
    SessionDeclined,
    ResumeConfirmationRequired,
    SessionTimedOutRequiresConfirmation,
    ExplicitTaskSelectionOverridesSession,
    ResumeTargetMissing,
    ResumeTargetTerminal,
    RunnerSessionInvalid,
    MissingRunnerSessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResumeDecision {
    pub status: ResumeStatus,
    pub scope: ResumeScope,
    pub reason: ResumeReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub message: String,
    pub detail: String,
}

impl ResumeDecision {
    pub fn blocking_state(&self) -> Option<BlockingState> {
        let reason = match self.reason {
            ResumeReason::RunnerSessionInvalid => "runner_session_invalid",
            ResumeReason::MissingRunnerSessionId => "missing_runner_session_id",
            ResumeReason::ResumeConfirmationRequired => "resume_confirmation_required",
            ResumeReason::SessionTimedOutRequiresConfirmation => {
                "session_timed_out_requires_confirmation"
            }
            _ => return None,
        };

        Some(
            BlockingState::runner_recovery(
                match self.scope {
                    ResumeScope::RunSession => "run_session",
                    ResumeScope::ContinueSession => "continue_session",
                },
                reason,
                self.task_id.clone(),
                self.message.clone(),
                self.detail.clone(),
            )
            .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback()),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeBehavior {
    Prompt,
    AutoResume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeDecisionMode {
    Preview,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeResolution {
    pub resume_task_id: Option<String>,
    pub completed_count: u32,
    pub decision: Option<ResumeDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSessionDecisionOptions<'a> {
    pub timeout_hours: Option<u64>,
    pub behavior: ResumeBehavior,
    pub non_interactive: bool,
    pub explicit_task_id: Option<&'a str>,
    pub announce_missing_session: bool,
    pub mode: ResumeDecisionMode,
}

pub fn resolve_run_session_decision(
    cache_dir: &Path,
    queue_file: &QueueFile,
    options: RunSessionDecisionOptions<'_>,
) -> Result<ResumeResolution> {
    let validation = check_session(cache_dir, queue_file, options.timeout_hours)?;
    let can_prompt = !options.non_interactive && std::io::stdin().is_terminal();
    let timeout_threshold = options
        .timeout_hours
        .unwrap_or(crate::constants::timeouts::DEFAULT_SESSION_TIMEOUT_HOURS);

    let resolution = match validation {
        SessionValidationResult::NoSession => ResumeResolution {
            resume_task_id: None,
            completed_count: 0,
            decision: options.announce_missing_session.then(|| ResumeDecision {
                status: ResumeStatus::FallingBackToFreshInvocation,
                scope: ResumeScope::RunSession,
                reason: ResumeReason::NoSession,
                task_id: None,
                message: "Resume: no interrupted session was found; starting a fresh run."
                    .to_string(),
                detail: "No persisted session state exists under .ralph/cache/session.jsonc."
                    .to_string(),
            }),
        },
        SessionValidationResult::Valid(session) => {
            if let Some(explicit_task_id) = options.explicit_task_id
                && explicit_task_id.trim() != session.task_id
            {
                ResumeResolution {
                    resume_task_id: None,
                    completed_count: 0,
                    decision: Some(ResumeDecision {
                        status: ResumeStatus::FallingBackToFreshInvocation,
                        scope: ResumeScope::RunSession,
                        reason: ResumeReason::ExplicitTaskSelectionOverridesSession,
                        task_id: Some(session.task_id.clone()),
                        message: format!(
                            "Resume: starting fresh because task {explicit_task_id} was explicitly selected instead of interrupted task {}.",
                            session.task_id
                        ),
                        detail: format!(
                            "Saved session belongs to {}, so Ralph will honor the explicit task selection.",
                            session.task_id
                        ),
                    }),
                }
            } else {
                match options.behavior {
                    ResumeBehavior::AutoResume => ResumeResolution {
                        resume_task_id: Some(session.task_id.clone()),
                        completed_count: session.tasks_completed_in_loop,
                        decision: Some(ResumeDecision {
                            status: ResumeStatus::ResumingSameSession,
                            scope: ResumeScope::RunSession,
                            reason: ResumeReason::SessionValid,
                            task_id: Some(session.task_id.clone()),
                            message: format!(
                                "Resume: continuing the interrupted session for task {}.",
                                session.task_id
                            ),
                            detail: format!(
                                "Saved session is current and will resume from phase {} with {} completed loop task(s).",
                                session.current_phase, session.tasks_completed_in_loop
                            ),
                        }),
                    },
                    ResumeBehavior::Prompt if !can_prompt => ResumeResolution {
                        resume_task_id: None,
                        completed_count: 0,
                        decision: Some(ResumeDecision {
                            status: ResumeStatus::RefusingToResume,
                            scope: ResumeScope::RunSession,
                            reason: ResumeReason::ResumeConfirmationRequired,
                            task_id: Some(session.task_id.clone()),
                            message: format!(
                                "Resume: refusing to guess because task {} has an interrupted session and confirmation is unavailable.",
                                session.task_id
                            ),
                            detail: "Re-run interactively to choose resume vs fresh, or pass --resume to continue automatically when safe.".to_string(),
                        }),
                    },
                    ResumeBehavior::Prompt => {
                        if prompt_session_recovery(&session, options.non_interactive)? {
                            ResumeResolution {
                                resume_task_id: Some(session.task_id.clone()),
                                completed_count: session.tasks_completed_in_loop,
                                decision: Some(ResumeDecision {
                                    status: ResumeStatus::ResumingSameSession,
                                    scope: ResumeScope::RunSession,
                                    reason: ResumeReason::SessionValid,
                                    task_id: Some(session.task_id.clone()),
                                    message: format!(
                                        "Resume: continuing the interrupted session for task {}.",
                                        session.task_id
                                    ),
                                    detail: format!(
                                        "Saved session is current and will resume from phase {} with {} completed loop task(s).",
                                        session.current_phase, session.tasks_completed_in_loop
                                    ),
                                }),
                            }
                        } else {
                            maybe_clear_session(cache_dir, options.mode)?;
                            ResumeResolution {
                                resume_task_id: None,
                                completed_count: 0,
                                decision: Some(ResumeDecision {
                                    status: ResumeStatus::FallingBackToFreshInvocation,
                                    scope: ResumeScope::RunSession,
                                    reason: ResumeReason::SessionDeclined,
                                    task_id: Some(session.task_id.clone()),
                                    message: format!(
                                        "Resume: starting fresh after declining the interrupted session for task {}.",
                                        session.task_id
                                    ),
                                    detail: "The saved session remains readable, but Ralph will begin a new invocation instead of reusing it.".to_string(),
                                }),
                            }
                        }
                    }
                }
            }
        }
        SessionValidationResult::Stale { reason } => fresh_start_resolution(
            cache_dir,
            options.mode,
            ResumeReason::SessionStale,
            None,
            "Resume: starting fresh because the saved session is stale.".to_string(),
            reason,
        )?,
        SessionValidationResult::Timeout { hours, session } => {
            if !can_prompt {
                ResumeResolution {
                    resume_task_id: None,
                    completed_count: 0,
                    decision: Some(ResumeDecision {
                        status: ResumeStatus::RefusingToResume,
                        scope: ResumeScope::RunSession,
                        reason: ResumeReason::SessionTimedOutRequiresConfirmation,
                        task_id: Some(session.task_id.clone()),
                        message: format!(
                            "Resume: refusing to continue timed-out session {} without explicit confirmation.",
                            session.task_id
                        ),
                        detail: format!(
                            "The saved session is {hours} hour(s) old, exceeding the configured {timeout_threshold}-hour safety threshold."
                        ),
                    }),
                }
            } else if prompt_session_recovery_timeout(
                &session,
                hours,
                timeout_threshold,
                options.non_interactive,
            )? {
                ResumeResolution {
                    resume_task_id: Some(session.task_id.clone()),
                    completed_count: session.tasks_completed_in_loop,
                    decision: Some(ResumeDecision {
                        status: ResumeStatus::ResumingSameSession,
                        scope: ResumeScope::RunSession,
                        reason: ResumeReason::SessionTimedOutConfirmed,
                        task_id: Some(session.task_id.clone()),
                        message: format!(
                            "Resume: continuing timed-out session {} after explicit confirmation.",
                            session.task_id
                        ),
                        detail: format!(
                            "The saved session is {hours} hour(s) old, above the configured {timeout_threshold}-hour threshold."
                        ),
                    }),
                }
            } else {
                fresh_start_resolution(
                    cache_dir,
                    options.mode,
                    ResumeReason::SessionDeclined,
                    Some(session.task_id.clone()),
                    format!(
                        "Resume: starting fresh after declining timed-out session {}.",
                        session.task_id
                    ),
                    format!(
                        "The saved session is {hours} hour(s) old, above the configured {timeout_threshold}-hour threshold."
                    ),
                )?
            }
        }
    };

    Ok(resolution)
}

fn maybe_clear_session(cache_dir: &Path, mode: ResumeDecisionMode) -> Result<()> {
    if matches!(mode, ResumeDecisionMode::Execute) {
        clear_session(cache_dir)?;
    }
    Ok(())
}

fn fresh_start_resolution(
    cache_dir: &Path,
    mode: ResumeDecisionMode,
    reason: ResumeReason,
    task_id: Option<String>,
    message: String,
    detail: String,
) -> Result<ResumeResolution> {
    maybe_clear_session(cache_dir, mode)?;
    Ok(ResumeResolution {
        resume_task_id: None,
        completed_count: 0,
        decision: Some(ResumeDecision {
            status: ResumeStatus::FallingBackToFreshInvocation,
            scope: ResumeScope::RunSession,
            reason,
            task_id,
            message,
            detail,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, SessionState, Task, TaskPriority, TaskStatus};
    use crate::session::{save_session, session_exists};

    fn test_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: Default::default(),
            parent_id: None,
            estimated_minutes: None,
            actual_minutes: None,
        }
    }

    fn test_session(task_id: &str) -> SessionState {
        SessionState::new(
            "test-session-id".to_string(),
            task_id.to_string(),
            crate::timeutil::now_utc_rfc3339_or_fallback(),
            1,
            crate::contracts::Runner::Claude,
            "sonnet".to_string(),
            0,
            None,
            None,
        )
    }

    #[test]
    fn fresh_start_resolution_preview_keeps_session_cache() {
        let temp_dir = tempfile::TempDir::new().expect("tempdir");
        let session = test_session("RQ-0001");
        save_session(temp_dir.path(), &session).expect("save session");

        let resolution = fresh_start_resolution(
            temp_dir.path(),
            ResumeDecisionMode::Preview,
            ResumeReason::SessionDeclined,
            Some("RQ-0001".to_string()),
            "preview".to_string(),
            "detail".to_string(),
        )
        .expect("resolution");

        assert!(session_exists(temp_dir.path()));
        assert_eq!(
            resolution.decision.expect("decision").reason,
            ResumeReason::SessionDeclined
        );
    }

    #[test]
    fn fresh_start_resolution_execute_clears_session_cache() {
        let temp_dir = tempfile::TempDir::new().expect("tempdir");
        let session = test_session("RQ-0001");
        save_session(temp_dir.path(), &session).expect("save session");

        fresh_start_resolution(
            temp_dir.path(),
            ResumeDecisionMode::Execute,
            ResumeReason::SessionDeclined,
            Some("RQ-0001".to_string()),
            "execute".to_string(),
            "detail".to_string(),
        )
        .expect("resolution");

        assert!(!session_exists(temp_dir.path()));
    }

    #[test]
    fn maybe_clear_session_preview_is_noop() {
        let temp_dir = tempfile::TempDir::new().expect("tempdir");
        let session = test_session("RQ-0001");
        save_session(temp_dir.path(), &session).expect("save session");

        maybe_clear_session(temp_dir.path(), ResumeDecisionMode::Preview).expect("clear preview");

        assert!(session_exists(temp_dir.path()));
    }

    #[test]
    fn resolve_run_session_decision_announces_missing_session_when_requested() {
        let temp_dir = tempfile::TempDir::new().expect("tempdir");
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Todo)],
        };

        let resolution = resolve_run_session_decision(
            temp_dir.path(),
            &queue,
            RunSessionDecisionOptions {
                timeout_hours: Some(24),
                behavior: ResumeBehavior::AutoResume,
                non_interactive: true,
                explicit_task_id: None,
                announce_missing_session: true,
                mode: ResumeDecisionMode::Execute,
            },
        )
        .expect("resolution");

        let decision = resolution.decision.expect("decision");
        assert_eq!(decision.status, ResumeStatus::FallingBackToFreshInvocation);
        assert_eq!(decision.reason, ResumeReason::NoSession);
    }
}
