//! Daemon command implementation for background service management.
//!
//! Purpose:
//! - Daemon command implementation for background service management.
//!
//! Responsibilities:
//! - Re-export daemon subcommands (start, stop, serve, status, logs)
//! - Define shared types (DaemonState) and constants
//! - Provide shared helpers for daemon state management and lifecycle coordination
//!
//! Not handled here:
//! - Individual command implementations (see submodules)
//! - Windows service management (Unix-only implementation)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Daemon uses a dedicated lock at `.ralph/cache/daemon.lock`
//! - Daemon state is stored at `.ralph/cache/daemon.json`
//! - Startup serialization uses a separate `.ralph/cache/daemon.start.lock`

mod logs;
mod serve;
mod start;
mod status;
mod stop;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Child;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub use logs::logs;
pub use serve::serve;
pub use start::start;
pub use status::status;
pub use stop::stop;

/// Daemon state file name.
pub(super) const DAEMON_STATE_FILE: &str = "daemon.json";
/// Daemon readiness file name.
pub(super) const DAEMON_READY_FILE: &str = "daemon.ready";
/// Daemon lock directory name (relative to .ralph/cache).
pub(super) const DAEMON_LOCK_DIR: &str = "daemon.lock";
/// Daemon startup lock directory name (relative to .ralph/cache).
pub(super) const DAEMON_START_LOCK_DIR: &str = "daemon.start.lock";

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

struct DaemonCacheWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<notify::Result<notify::Event>>,
}

impl DaemonCacheWatcher {
    fn new(cache_dir: &Path) -> Result<Self> {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

        std::fs::create_dir_all(cache_dir).with_context(|| {
            format!("Failed to create daemon cache dir {}", cache_dir.display())
        })?;

        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )
        .context("Failed to create daemon cache watcher")?;
        watcher
            .watch(cache_dir, RecursiveMode::NonRecursive)
            .with_context(|| format!("Failed to watch daemon cache dir {}", cache_dir.display()))?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    fn recv_timeout(&self, timeout: Duration) -> bool {
        self.rx.recv_timeout(timeout).is_ok()
    }
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

fn daemon_ready_path(cache_dir: &Path) -> std::path::PathBuf {
    cache_dir.join(DAEMON_READY_FILE)
}

fn daemon_state_path(cache_dir: &Path) -> std::path::PathBuf {
    cache_dir.join(DAEMON_STATE_FILE)
}

fn daemon_lock_path(cache_dir: &Path) -> std::path::PathBuf {
    cache_dir.join(DAEMON_LOCK_DIR)
}

pub(super) fn write_daemon_ready(cache_dir: &Path, pid: u32) -> Result<()> {
    let path = daemon_ready_path(cache_dir);
    crate::fsutil::write_atomic(&path, format!("{pid}\n").as_bytes())
        .with_context(|| format!("Failed to write daemon ready marker to {}", path.display()))?;
    Ok(())
}

fn daemon_ready_matches_pid(cache_dir: &Path, pid: u32) -> Result<bool> {
    let path = daemon_ready_path(cache_dir);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(anyhow::Error::from(err))
                .with_context(|| format!("Failed to read daemon ready marker {}", path.display()));
        }
    };

    let observed = raw.trim().parse::<u32>().with_context(|| {
        format!(
            "Failed to parse daemon ready marker {} as a PID",
            path.display()
        )
    })?;
    Ok(observed == pid)
}

fn remove_daemon_file(path: &Path, description: &str) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        log::debug!(
            "Failed to remove {description} {}: {}",
            path.display(),
            error
        );
    }
}

fn remove_daemon_dir(path: &Path, description: &str) {
    if let Err(error) = fs::remove_dir_all(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        log::debug!(
            "Failed to remove {description} {}: {}",
            path.display(),
            error
        );
    }
}

pub(super) fn clear_daemon_runtime_artifacts(cache_dir: &Path, remove_lock: bool) {
    remove_daemon_file(&daemon_state_path(cache_dir), "daemon state file");
    remove_daemon_file(&daemon_ready_path(cache_dir), "daemon ready marker");
    if remove_lock {
        remove_daemon_dir(&daemon_lock_path(cache_dir), "daemon lock dir");
    }
}

fn daemon_shutdown_complete(cache_dir: &Path, pid: u32) -> bool {
    matches!(
        daemon_pid_liveness(pid),
        crate::lock::PidLiveness::NotRunning
    ) || (!daemon_state_path(cache_dir).exists()
        && !daemon_ready_path(cache_dir).exists()
        && !daemon_lock_path(cache_dir).exists())
}

/// Wait for the daemon to publish its explicit ready marker or exit early.
pub(super) fn wait_for_daemon_ready(
    cache_dir: &Path,
    pid: u32,
    timeout: Duration,
    child: &mut Child,
) -> Result<bool> {
    let watcher = DaemonCacheWatcher::new(cache_dir).ok();
    let deadline = Instant::now() + timeout;
    loop {
        if daemon_ready_matches_pid(cache_dir, pid)? {
            return Ok(true);
        }
        if child
            .try_wait()
            .with_context(|| format!("Failed to inspect daemon child {pid}"))?
            .is_some()
        {
            return Ok(false);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        let wait_slice = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(100))
            .max(Duration::from_millis(1));
        if let Some(ref watcher) = watcher {
            let _ = watcher.recv_timeout(wait_slice);
        } else {
            std::thread::park_timeout(wait_slice);
        }
    }
}

/// Wait for the daemon to exit and release its runtime artifacts.
pub(super) fn wait_for_daemon_shutdown(
    cache_dir: &Path,
    pid: u32,
    timeout: Duration,
) -> Result<bool> {
    let watcher = DaemonCacheWatcher::new(cache_dir).ok();
    let deadline = Instant::now() + timeout;
    loop {
        if daemon_shutdown_complete(cache_dir, pid) {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        let wait_slice = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(100))
            .max(Duration::from_millis(1));
        if let Some(ref watcher) = watcher {
            let _ = watcher.recv_timeout(wait_slice);
        } else {
            std::thread::park_timeout(wait_slice);
        }
    }
}

/// Check PID liveness for daemon processes.
pub(super) fn daemon_pid_liveness(pid: u32) -> crate::lock::PidLiveness {
    crate::lock::pid_liveness(pid)
}

/// Render manual cleanup instructions for stale/indeterminate daemon state.
pub(super) fn manual_daemon_cleanup_instructions(cache_dir: &Path) -> String {
    format!(
        "If you are certain the daemon is stopped, manually remove:\n  rm {}\n  rm {}\n  rm -rf {}",
        cache_dir.join(DAEMON_STATE_FILE).display(),
        cache_dir.join(DAEMON_READY_FILE).display(),
        cache_dir.join(DAEMON_LOCK_DIR).display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use tempfile::TempDir;

    fn deterministic_non_running_pid() -> u32 {
        const MAX_SAFE_PID: u32 = i32::MAX as u32;
        for offset in 0..=1024 {
            let candidate = MAX_SAFE_PID - offset;
            if crate::lock::pid_is_running(candidate) == Some(false) {
                return candidate;
            }
        }

        panic!("failed to find a deterministic non-running PID candidate");
    }

    #[test]
    fn wait_for_daemon_ready_returns_true_when_marker_appears() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        fs::create_dir_all(&cache_dir).expect("create cache dir");
        let expected_pid = 424_242_u32;

        let writer_cache_dir = cache_dir.clone();
        let writer = std::thread::spawn(move || {
            std::thread::park_timeout(Duration::from_millis(60));
            write_daemon_ready(&writer_cache_dir, expected_pid).expect("write daemon ready");
        });

        let mut child = Command::new("python3")
            .arg("-c")
            .arg("import time; time.sleep(5)")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn helper child");

        let ready =
            wait_for_daemon_ready(&cache_dir, expected_pid, Duration::from_secs(1), &mut child)
                .expect("wait for daemon ready");
        writer.join().expect("join writer thread");
        let _ = child.kill();
        let _ = child.wait();
        assert!(ready, "expected daemon state to appear before timeout");
    }

    #[test]
    fn wait_for_daemon_ready_returns_false_when_child_exits() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        fs::create_dir_all(&cache_dir).expect("create cache dir");

        let mut child = Command::new("python3")
            .arg("-c")
            .arg("print('boom')")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn helper child");

        let ready =
            wait_for_daemon_ready(&cache_dir, 123_456_u32, Duration::from_secs(1), &mut child)
                .expect("wait for daemon ready");
        let mut stdout = String::new();
        child
            .stdout
            .take()
            .expect("capture child stdout")
            .read_to_string(&mut stdout)
            .expect("read child stdout");
        assert!(!ready, "expected early failure when daemon child exits");
        assert!(stdout.contains("boom"));
    }

    #[test]
    fn wait_for_daemon_shutdown_returns_true_after_artifacts_clear() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        fs::create_dir_all(&cache_dir).expect("create cache dir");
        let pid = deterministic_non_running_pid();

        write_daemon_state(
            &cache_dir,
            &DaemonState {
                version: 1,
                pid,
                started_at: "2026-01-01T00:00:00Z".to_string(),
                repo_root: "/tmp/repo".to_string(),
                command: "ralph daemon serve".to_string(),
            },
        )
        .expect("write daemon state");
        write_daemon_ready(&cache_dir, pid).expect("write daemon ready");
        fs::create_dir_all(cache_dir.join(DAEMON_LOCK_DIR)).expect("create daemon lock dir");

        clear_daemon_runtime_artifacts(&cache_dir, true);

        let ready = wait_for_daemon_shutdown(&cache_dir, pid, Duration::from_secs(1))
            .expect("wait for daemon shutdown");
        assert!(
            ready,
            "expected daemon shutdown check to observe cleared artifacts"
        );
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
            "rm {}",
            cache_dir.join(DAEMON_READY_FILE).display()
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
