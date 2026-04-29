//! Session file persistence helpers.
//!
//! Purpose:
//! - Session file persistence helpers.
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
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Session files are written atomically.
//! - Forward-version session files log a warning and still attempt to load.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::constants::paths::SESSION_FILENAME;
use crate::contracts::SessionState;
use crate::fsutil;
use crate::git::error::git_head_commit;

const SESSION_QUARANTINE_DIR: &str = "session-quarantine";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCacheCorruption {
    pub path: PathBuf,
    pub diagnostic: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionQuarantineResult {
    pub original_path: PathBuf,
    pub quarantine_path: PathBuf,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLoadResult {
    Missing,
    Loaded(SessionState),
    Corrupt(SessionCacheCorruption),
}

fn session_cache_corruption(
    path: &Path,
    context: &str,
    err: &(dyn std::error::Error + 'static),
) -> SessionCacheCorruption {
    SessionCacheCorruption {
        path: path.to_path_buf(),
        diagnostic: crate::redaction::redact_text(&format!("{context}: {err:#}")),
    }
}

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

/// Load session state from disk, returning a recoverable corruption classification.
pub fn load_session_checked(cache_dir: &Path) -> SessionLoadResult {
    let path = session_path(cache_dir);
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return SessionLoadResult::Missing,
        Err(err) => {
            return SessionLoadResult::Corrupt(session_cache_corruption(
                &path,
                "inspect session file",
                &err,
            ));
        }
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            return SessionLoadResult::Corrupt(session_cache_corruption(
                &path,
                "read session file",
                &err,
            ));
        }
    };

    match serde_json::from_str::<SessionState>(&content) {
        Ok(session) => {
            if session.version > crate::contracts::SESSION_STATE_VERSION {
                log::warn!(
                    "Session file version {} is newer than supported version {}. Attempting to load anyway.",
                    session.version,
                    crate::contracts::SESSION_STATE_VERSION
                );
            }
            SessionLoadResult::Loaded(session)
        }
        Err(err) => {
            SessionLoadResult::Corrupt(session_cache_corruption(&path, "parse session file", &err))
        }
    }
}

/// Load session state from disk.
pub fn load_session(cache_dir: &Path) -> Result<Option<SessionState>> {
    match load_session_checked(cache_dir) {
        SessionLoadResult::Missing => Ok(None),
        SessionLoadResult::Loaded(session) => Ok(Some(session)),
        SessionLoadResult::Corrupt(corruption) => {
            bail!("{}: {}", corruption.path.display(), corruption.diagnostic)
        }
    }
}

/// Move a corrupt session cache to a diagnostics-preserving quarantine location.
pub fn quarantine_session_cache(cache_dir: &Path) -> Result<Option<SessionQuarantineResult>> {
    let original_path = session_path(cache_dir);
    if !original_path.exists() {
        return Ok(None);
    }

    let quarantine_dir = cache_dir.join(SESSION_QUARANTINE_DIR);
    std::fs::create_dir_all(&quarantine_dir).with_context(|| {
        format!(
            "create session quarantine directory {}",
            quarantine_dir.display()
        )
    })?;

    let timestamp = crate::timeutil::now_utc_rfc3339_or_fallback().replace([':', '.'], "-");
    let quarantine_path = quarantine_dir.join(format!("session.jsonc.corrupt.{timestamp}"));

    match std::fs::rename(&original_path, &quarantine_path) {
        Ok(()) => {}
        Err(rename_err) => {
            std::fs::copy(&original_path, &quarantine_path).with_context(|| {
                format!("quarantine session cache to {}", quarantine_path.display())
            })?;
            std::fs::remove_file(&original_path).with_context(|| {
                format!(
                    "remove quarantined session cache {}",
                    original_path.display()
                )
            })?;
            log::debug!(
                "session cache rename failed during quarantine; used copy/remove fallback: {rename_err}"
            );
        }
    }

    Ok(Some(SessionQuarantineResult {
        original_path,
        quarantine_path,
    }))
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
