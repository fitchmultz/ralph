//! Session loop-progress mutation helpers.
//!
//! Purpose:
//! - Session loop-progress mutation helpers.
//!
//! Responsibilities:
//! - Increment persisted loop progress without exposing persistence details to callers.
//!
//! Not handled here:
//! - Session validation.
//! - Recovery prompting.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Missing session files are a no-op.
//! - Progress updates reuse the same persistence path as session recovery.

use std::path::Path;

use anyhow::Result;

use super::persistence::{load_session, save_session};

/// Increment the session's tasks_completed_in_loop counter and persist.
pub fn increment_session_progress(cache_dir: &Path) -> Result<()> {
    let mut session = match load_session(cache_dir)? {
        Some(session) => session,
        None => {
            log::debug!("No session to increment progress for");
            return Ok(());
        }
    };

    let now = crate::timeutil::now_utc_rfc3339_or_fallback();
    session.mark_task_complete(now);
    save_session(cache_dir, &session)
}
