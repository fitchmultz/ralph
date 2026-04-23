//! Session file persistence helpers.
//!
//! Responsibilities:
//! - Read, write, clear, and locate `.ralph/cache/session.jsonc`.
//! - Resolve git HEAD metadata used by session tracking.
//!
//! Not handled here:
//! - Session validation logic.
//! - Interactive recovery prompts.
//! - Loop-progress mutation.
//!
//! Invariants/assumptions:
//! - Session files are written atomically.
//! - Forward-version session files log a warning and still attempt to load.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::constants::paths::SESSION_FILENAME;
use crate::contracts::SessionState;
use crate::fsutil;
use crate::git::error::git_head_commit;

/// Get the path to the session file.
pub fn session_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(SESSION_FILENAME)
}

/// Check if a session file exists.
pub fn session_exists(cache_dir: &Path) -> bool {
    session_path(cache_dir).exists()
}

/// Save session state to disk.
pub fn save_session(cache_dir: &Path, session: &SessionState) -> Result<()> {
    let path = session_path(cache_dir);
    let json = serde_json::to_string_pretty(session).context("serialize session state")?;
    fsutil::write_atomic(&path, json.as_bytes()).context("write session file")?;
    log::debug!("Session saved: task_id={}", session.task_id);
    Ok(())
}

/// Load session state from disk.
pub fn load_session(cache_dir: &Path) -> Result<Option<SessionState>> {
    let path = session_path(cache_dir);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path).context("read session file")?;
    let session: SessionState = serde_json::from_str(&content).context("parse session file")?;

    if session.version > crate::contracts::SESSION_STATE_VERSION {
        log::warn!(
            "Session file version {} is newer than supported version {}. Attempting to load anyway.",
            session.version,
            crate::contracts::SESSION_STATE_VERSION
        );
    }

    Ok(Some(session))
}

/// Clear (delete) the session file.
pub fn clear_session(cache_dir: &Path) -> Result<()> {
    let path = session_path(cache_dir);
    if path.exists() {
        std::fs::remove_file(&path).context("remove session file")?;
        log::debug!("Session cleared");
    }
    Ok(())
}

/// Get the git HEAD commit hash for session tracking.
pub fn get_git_head_commit(repo_root: &Path) -> Option<String> {
    git_head_commit(repo_root).ok()
}
