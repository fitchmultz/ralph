//! Daemon command implementation for background service management.
//!
//! Responsibilities:
//! - Re-export daemon subcommands (start, stop, serve, status, logs)
//! - Define shared types (DaemonState) and constants
//! - Provide shared helpers for daemon state management
//!
//! Not handled here:
//! - Individual command implementations (see submodules)
//! - Windows service management (Unix-only implementation)
//!
//! Invariants/assumptions:
//! - Daemon uses a dedicated lock at `.ralph/cache/daemon.lock`
//! - Daemon state is stored at `.ralph/cache/daemon.json`

mod logs;
mod serve;
mod start;
mod status;
mod stop;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

pub use logs::logs;
pub use serve::serve;
pub use start::start;
pub use status::status;
pub use stop::stop;

/// Daemon state file name.
pub(super) const DAEMON_STATE_FILE: &str = "daemon.json";
/// Daemon lock directory name (relative to .ralph/cache).
pub(super) const DAEMON_LOCK_DIR: &str = "daemon.lock";

/// Re-export for use in submodules.
pub(super) use logs::DAEMON_LOG_FILE_NAME;

/// Daemon state persisted to disk.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct DaemonState {
    /// Schema version for future compatibility.
    pub(super) version: u32,
    /// Process ID of the daemon.
    pub(super) pid: u32,
    /// ISO 8601 timestamp when the daemon started.
    pub(super) started_at: String,
    /// Repository root path.
    pub(super) repo_root: String,
    /// Full command line of the daemon process.
    pub(super) command: String,
}

/// Read daemon state from disk.
pub(super) fn get_daemon_state(cache_dir: &Path) -> Result<Option<DaemonState>> {
    let path = cache_dir.join(DAEMON_STATE_FILE);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read daemon state from {}", path.display()))?;

    let state: DaemonState = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse daemon state from {}", path.display()))?;

    Ok(Some(state))
}

/// Write daemon state to disk atomically.
pub(super) fn write_daemon_state(cache_dir: &Path, state: &DaemonState) -> Result<()> {
    let path = cache_dir.join(DAEMON_STATE_FILE);
    let content =
        serde_json::to_string_pretty(state).context("Failed to serialize daemon state")?;
    crate::fsutil::write_atomic(&path, content.as_bytes())
        .with_context(|| format!("Failed to write daemon state to {}", path.display()))?;
    Ok(())
}

/// Poll daemon state until it matches `pid` or a timeout elapses.
pub(super) fn wait_for_daemon_state_pid(
    cache_dir: &Path,
    pid: u32,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<bool> {
    let poll_interval = poll_interval.max(Duration::from_millis(1));
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(state) = get_daemon_state(cache_dir)?
            && state.pid == pid
        {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(poll_interval);
    }
}

/// Check PID liveness for daemon processes.
pub(super) fn daemon_pid_liveness(pid: u32) -> crate::lock::PidLiveness {
    crate::lock::pid_liveness(pid)
}

/// Render manual cleanup instructions for stale/indeterminate daemon state.
pub(super) fn manual_daemon_cleanup_instructions(cache_dir: &Path) -> String {
    format!(
        "If you are certain the daemon is stopped, manually remove:\n  rm {}\n  rm -rf {}",
        cache_dir.join(DAEMON_STATE_FILE).display(),
        cache_dir.join(DAEMON_LOCK_DIR).display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn wait_for_daemon_state_pid_returns_true_when_state_appears() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        fs::create_dir_all(&cache_dir).expect("create cache dir");
        let expected_pid = 424_242_u32;

        let writer_cache_dir = cache_dir.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            let state = DaemonState {
                version: 1,
                pid: expected_pid,
                started_at: "2026-01-01T00:00:00Z".to_string(),
                repo_root: "/tmp/repo".to_string(),
                command: "ralph daemon serve".to_string(),
            };
            write_daemon_state(&writer_cache_dir, &state).expect("write daemon state");
        });

        let ready = wait_for_daemon_state_pid(
            &cache_dir,
            expected_pid,
            Duration::from_secs(1),
            Duration::from_millis(10),
        )
        .expect("poll daemon state");
        writer.join().expect("join writer thread");
        assert!(ready, "expected daemon state to appear before timeout");
    }

    #[test]
    fn wait_for_daemon_state_pid_returns_false_on_timeout() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        fs::create_dir_all(&cache_dir).expect("create cache dir");

        let ready = wait_for_daemon_state_pid(
            &cache_dir,
            123_456_u32,
            Duration::from_millis(100),
            Duration::from_millis(10),
        )
        .expect("poll daemon state");
        assert!(!ready, "expected timeout when daemon state is absent");
    }

    #[test]
    fn manual_cleanup_instructions_include_state_and_lock_paths() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        let instructions = manual_daemon_cleanup_instructions(&cache_dir);

        assert!(instructions.contains(&format!(
            "rm {}",
            cache_dir.join(DAEMON_STATE_FILE).display()
        )));
        assert!(instructions.contains(&format!(
            "rm -rf {}",
            cache_dir.join(DAEMON_LOCK_DIR).display()
        )));
    }

    #[test]
    fn manual_cleanup_instructions_do_not_reference_force_flag() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        let instructions = manual_daemon_cleanup_instructions(&cache_dir);

        assert!(
            !instructions.contains("--force"),
            "daemon cleanup instructions must not mention nonexistent --force flag"
        );
    }
}
