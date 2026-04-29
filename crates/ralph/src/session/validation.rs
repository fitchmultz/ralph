//! Session validation and classification helpers.
//!
//! Purpose:
//! - Session validation and classification helpers.
//!
//! Responsibilities:
//! - Validate persisted sessions against queue state and timeout policy.
//! - Return explicit recovery classifications for callers.
//!
//! Not handled here:
//! - Session persistence.
//! - Interactive recovery prompts.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Sessions are resumable only when the task still exists and is `Doing`.
//! - Timeout checks use `last_updated_at` when it parses successfully.

use std::path::Path;

use crate::contracts::{QueueFile, SessionState, TaskStatus};
use crate::timeutil;

use super::persistence::{SessionCacheCorruption, SessionLoadResult, load_session_checked};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionValidationResult {
    Valid(SessionState),
    NoSession,
    Stale { reason: String },
    Timeout { hours: u64, session: SessionState },
    CorruptCache(SessionCacheCorruption),
}

pub fn validate_session_with_now(
    session: &SessionState,
    queue: &QueueFile,
    timeout_hours: Option<u64>,
    now: time::OffsetDateTime,
) -> SessionValidationResult {
    let task = match queue
        .tasks
        .iter()
        .find(|task| task.id.trim() == session.task_id)
    {
        Some(task) => task,
        None => {
            return SessionValidationResult::Stale {
                reason: format!("Task {} no longer exists in queue", session.task_id),
            };
        }
    };

    if task.status != TaskStatus::Doing {
        return SessionValidationResult::Stale {
            reason: format!(
                "Task {} is not in Doing status (current: {})",
                session.task_id, task.status
            ),
        };
    }

    if let Some(timeout) = timeout_hours
        && let Ok(session_time) = timeutil::parse_rfc3339(&session.last_updated_at)
        && now > session_time
    {
        let elapsed = now - session_time;
        let hours = elapsed.whole_hours() as u64;
        if hours >= timeout {
            return SessionValidationResult::Timeout {
                hours,
                session: session.clone(),
            };
        }
    }

    SessionValidationResult::Valid(session.clone())
}

pub fn validate_session(
    session: &SessionState,
    queue: &QueueFile,
    timeout_hours: Option<u64>,
) -> SessionValidationResult {
    validate_session_with_now(
        session,
        queue,
        timeout_hours,
        time::OffsetDateTime::now_utc(),
    )
}

pub fn check_session(
    cache_dir: &Path,
    queue: &QueueFile,
    timeout_hours: Option<u64>,
) -> anyhow::Result<SessionValidationResult> {
    let session = match load_session_checked(cache_dir) {
        SessionLoadResult::Loaded(session) => session,
        SessionLoadResult::Missing => return Ok(SessionValidationResult::NoSession),
        SessionLoadResult::Corrupt(corruption) => {
            return Ok(SessionValidationResult::CorruptCache(corruption));
        }
    };

    Ok(validate_session(&session, queue, timeout_hours))
}
