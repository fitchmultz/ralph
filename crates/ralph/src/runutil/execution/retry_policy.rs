//! Retry admission helpers for runner execution.
//!
//! Purpose:
//! - Retry admission helpers for runner execution.
//!
//! Responsibilities:
//! - Decide whether a transient runner failure is safe to retry with the current repo state.
//!
//! Not handled here:
//! - Backoff scheduling.
//! - Continue-session flows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Retry is skipped if repo cleanliness cannot be established reliably.

use std::path::Path;

use anyhow::Result;

use crate::contracts::GitRevertMode;

pub(super) fn should_retry_with_repo_state(
    repo_root: &Path,
    revert_on_error: bool,
    git_revert_mode: GitRevertMode,
) -> Result<bool> {
    let dirty_only_allowed = match crate::git::clean::repo_dirty_only_allowed_paths(
        repo_root,
        crate::git::clean::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    ) {
        Ok(value) => value,
        Err(err) => {
            log::warn!("Failed to check repo state for retry; skipping retry: {err}");
            return Ok(false);
        }
    };

    if dirty_only_allowed {
        return Ok(true);
    }

    if revert_on_error && git_revert_mode == GitRevertMode::Enabled {
        return Ok(true);
    }

    Ok(false)
}
