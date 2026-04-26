//! Stop signal handling for graceful run loop termination.
//!
//! Purpose:
//! - Stop signal handling for graceful run loop termination.
//!
//! Responsibilities:
//! - Create and manage the stop signal file in the cache directory.
//! - Provide a simple file-based signaling mechanism for stopping the run loop.
//! - Ensure idempotent operations (creating when exists, clearing when absent).
//!
//! Not handled here:
//! - Signal delivery mechanisms (SIGINT, SIGTERM) - see `crate::runner` for that.
//! - Process-level signal handling or async notification.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The stop signal file path is `.ralph/cache/stop_requested`.
//! - Operations are synchronous and atomic where possible.
//! - Creating a signal when one exists is a no-op (overwrites with same content).
//! - Clearing a non-existent signal is a no-op.

use crate::constants::paths::STOP_SIGNAL_FILE;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopSignalSnapshot {
    pub path: PathBuf,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopSignalRequestResult {
    pub path: PathBuf,
    pub existed_before: bool,
    pub exists_after: bool,
}

/// Content written to the stop signal file (human-readable timestamp).
fn signal_content() -> String {
    format!(
        "Stop requested at {}",
        crate::timeutil::now_utc_rfc3339_or_fallback()
    )
}

/// Returns the full path to the stop signal file.
pub fn stop_signal_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(STOP_SIGNAL_FILE)
}

/// Capture the current stop-signal state without mutating it.
pub fn stop_signal_snapshot(cache_dir: &Path) -> StopSignalSnapshot {
    let path = stop_signal_path(cache_dir);
    StopSignalSnapshot {
        exists: path.exists(),
        path,
    }
}

/// Create the stop signal file in the cache directory.
///
/// This is idempotent - if the signal already exists, it will be overwritten
/// with a fresh timestamp.
pub fn create_stop_signal(cache_dir: &Path) -> Result<()> {
    let path = stop_signal_path(cache_dir);

    // Ensure cache directory exists
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("create cache directory {}", cache_dir.display()))?;

    // Write signal file atomically
    crate::fsutil::write_atomic(&path, signal_content().as_bytes())
        .with_context(|| format!("write stop signal file {}", path.display()))?;

    log::info!("Stop signal created at {}", path.display());
    Ok(())
}

/// Create the stop signal and return before/after metadata for machine callers.
pub fn request_stop_signal(cache_dir: &Path) -> Result<StopSignalRequestResult> {
    let existed_before = stop_signal_exists(cache_dir);
    create_stop_signal(cache_dir)?;
    let after = stop_signal_snapshot(cache_dir);
    Ok(StopSignalRequestResult {
        path: after.path,
        existed_before,
        exists_after: after.exists,
    })
}

/// Check if the stop signal file exists.
pub fn stop_signal_exists(cache_dir: &Path) -> bool {
    stop_signal_path(cache_dir).exists()
}

/// Remove the stop signal file (idempotent).
///
/// Returns `true` if a signal was actually removed, `false` if no signal existed.
pub fn clear_stop_signal(cache_dir: &Path) -> Result<bool> {
    let path = stop_signal_path(cache_dir);

    if !path.exists() {
        return Ok(false);
    }

    fs::remove_file(&path)
        .with_context(|| format!("remove stop signal file {}", path.display()))?;

    log::debug!("Stop signal cleared at {}", path.display());
    Ok(true)
}

/// Clear the stop signal at the start of a run loop.
///
/// This should be called before the loop begins to ensure a clean state,
/// handling the case where a previous run crashed and left a stale signal.
pub fn clear_stop_signal_at_loop_start(cache_dir: &Path) {
    match clear_stop_signal(cache_dir) {
        Ok(true) => {
            log::debug!("Cleared stale stop signal from previous run");
        }
        Ok(false) => {
            // No signal to clear - normal case
        }
        Err(e) => {
            // Log but don't fail - the loop should continue
            log::warn!("Failed to clear stop signal: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn stop_signal_path_construction() {
        let repo_root = crate::testsupport::path::portable_abs_path("signal-path-construction");
        let cache_dir = repo_root.join(".ralph/cache");
        let path = stop_signal_path(&cache_dir);
        assert_eq!(path, cache_dir.join("stop_requested"));
    }

    #[test]
    fn create_stop_signal_creates_file() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let cache_dir = temp.path().join("cache");

        assert!(!stop_signal_exists(&cache_dir));

        create_stop_signal(&cache_dir)?;

        assert!(stop_signal_exists(&cache_dir));
        let path = stop_signal_path(&cache_dir);
        let content = fs::read_to_string(&path)?;
        assert!(content.contains("Stop requested at"));

        Ok(())
    }

    #[test]
    fn stop_signal_snapshot_reports_current_state() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let cache_dir = temp.path().join("cache");

        let before = stop_signal_snapshot(&cache_dir);
        assert_eq!(before.path, cache_dir.join("stop_requested"));
        assert!(!before.exists);

        create_stop_signal(&cache_dir)?;
        let after = stop_signal_snapshot(&cache_dir);
        assert!(after.exists);

        Ok(())
    }

    #[test]
    fn request_stop_signal_reports_before_and_after_state() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let cache_dir = temp.path().join("cache");

        let created = request_stop_signal(&cache_dir)?;
        assert!(!created.existed_before);
        assert!(created.exists_after);
        assert_eq!(created.path, cache_dir.join("stop_requested"));

        let already_present = request_stop_signal(&cache_dir)?;
        assert!(already_present.existed_before);
        assert!(already_present.exists_after);

        Ok(())
    }

    #[test]
    fn create_stop_signal_is_idempotent() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let cache_dir = temp.path().join("cache");
        let path = stop_signal_path(&cache_dir);

        fs::create_dir_all(&cache_dir)?;
        fs::write(&path, "Stop requested at stale-timestamp")?;

        create_stop_signal(&cache_dir)?;
        let refreshed_content = fs::read_to_string(&path)?;

        // File should still exist and have updated content
        assert!(stop_signal_exists(&cache_dir));
        assert_ne!(refreshed_content, "Stop requested at stale-timestamp");
        assert!(refreshed_content.contains("Stop requested at"));

        Ok(())
    }

    #[test]
    fn clear_stop_signal_removes_file() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let cache_dir = temp.path().join("cache");

        create_stop_signal(&cache_dir)?;
        assert!(stop_signal_exists(&cache_dir));

        let cleared = clear_stop_signal(&cache_dir)?;

        assert!(cleared);
        assert!(!stop_signal_exists(&cache_dir));

        Ok(())
    }

    #[test]
    fn clear_stop_signal_is_idempotent() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let cache_dir = temp.path().join("cache");

        // Clearing non-existent signal should return Ok(false)
        let cleared = clear_stop_signal(&cache_dir)?;
        assert!(!cleared);

        Ok(())
    }

    #[test]
    fn clear_stop_signal_at_loop_start_handles_missing() {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().join("cache");

        // Should not panic or error when signal doesn't exist
        clear_stop_signal_at_loop_start(&cache_dir);

        assert!(!stop_signal_exists(&cache_dir));
    }

    #[test]
    fn clear_stop_signal_at_loop_start_clears_existing() {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().join("cache");

        create_stop_signal(&cache_dir).unwrap();
        assert!(stop_signal_exists(&cache_dir));

        clear_stop_signal_at_loop_start(&cache_dir);

        assert!(!stop_signal_exists(&cache_dir));
    }
}
