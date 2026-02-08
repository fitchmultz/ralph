//! Daemon command implementation for background service management.
//!
//! Responsibilities:
//! - Start/stop/status a background Ralph daemon process.
//! - Manage daemon state and lock files.
//! - Run the continuous execution loop in daemon mode.
//!
//! Not handled here:
//! - Windows service management (Unix-only implementation).
//! - Queue mutations (handled by `crate::queue`).
//!
//! Invariants/assumptions:
//! - Daemon uses a dedicated lock at `.ralph/cache/daemon.lock`.
//! - Daemon state is stored at `.ralph/cache/daemon.json`.
//! - The serve command is internal and should not be called directly by users.

use crate::cli::daemon::{DaemonServeArgs, DaemonStartArgs};
use crate::config::Resolved;
use crate::lock::{self, acquire_dir_lock};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Daemon state file name.
const DAEMON_STATE_FILE: &str = "daemon.json";
/// Daemon lock directory name (relative to .ralph/cache).
const DAEMON_LOCK_DIR: &str = "daemon.lock";

/// Daemon state persisted to disk.
#[derive(Debug, Serialize, Deserialize)]
struct DaemonState {
    /// Schema version for future compatibility.
    version: u32,
    /// Process ID of the daemon.
    pid: u32,
    /// ISO 8601 timestamp when the daemon started.
    started_at: String,
    /// Repository root path.
    repo_root: String,
    /// Full command line of the daemon process.
    command: String,
}

/// Start the daemon as a background process.
pub fn start(resolved: &Resolved, args: DaemonStartArgs) -> Result<()> {
    #[cfg(unix)]
    {
        let cache_dir = resolved.repo_root.join(".ralph/cache");
        let daemon_lock_dir = cache_dir.join(DAEMON_LOCK_DIR);

        // Check if daemon is already running
        if let Some(state) = get_daemon_state(&cache_dir)? {
            if is_pid_running(state.pid) {
                bail!(
                    "Daemon is already running (PID: {}). Use `ralph daemon stop` to stop it.",
                    state.pid
                );
            } else {
                log::warn!("Removing stale daemon state file");
                let _ = fs::remove_file(cache_dir.join(DAEMON_STATE_FILE));
            }
        }

        // Try to acquire the daemon lock to ensure no other daemon is starting
        let _lock = match acquire_dir_lock(&daemon_lock_dir, "daemon-start", false) {
            Ok(lock) => lock,
            Err(e) => {
                bail!(
                    "Failed to acquire daemon lock: {}. Another daemon may be starting.",
                    e
                );
            }
        };

        // Build the serve command
        let exe = std::env::current_exe().context("Failed to get current executable path")?;
        let mut command = std::process::Command::new(&exe);
        command
            .arg("daemon")
            .arg("serve")
            .arg("--empty-poll-ms")
            .arg(args.empty_poll_ms.to_string())
            .arg("--wait-poll-ms")
            .arg(args.wait_poll_ms.to_string());

        if args.notify_when_unblocked {
            command.arg("--notify-when-unblocked");
        }

        // Set up stdio redirection
        let log_dir = resolved.repo_root.join(".ralph/logs");
        fs::create_dir_all(&log_dir).context("Failed to create log directory")?;
        let log_file = std::fs::File::create(log_dir.join("daemon.log"))
            .context("Failed to create daemon log file")?;

        command
            .stdin(std::process::Stdio::null())
            .stdout(
                log_file
                    .try_clone()
                    .context("Failed to clone log file handle")?,
            )
            .stderr(log_file);

        // Detach from terminal on Unix
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }

        // Spawn the daemon process
        let child = command.spawn().context("Failed to spawn daemon process")?;
        let pid = child.id();

        if wait_for_daemon_state_pid(
            &cache_dir,
            pid,
            Duration::from_secs(2),
            Duration::from_millis(100),
        )? {
            println!("Daemon started successfully (PID: {})", pid);
            Ok(())
        } else {
            bail!("Daemon failed to start. Check .ralph/logs/daemon.log for details.");
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (resolved, args);
        bail!(
            "Daemon mode is only supported on Unix systems. Use `ralph run loop --continuous` in a terminal or configure a Windows service."
        );
    }
}

/// Stop the daemon gracefully.
pub fn stop(resolved: &Resolved) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");

    // Check if daemon is running
    let state = match get_daemon_state(&cache_dir)? {
        Some(state) => state,
        None => {
            println!("Daemon is not running");
            return Ok(());
        }
    };

    if !is_pid_running(state.pid) {
        println!("Daemon is not running (removing stale state file)");
        let _ = fs::remove_file(cache_dir.join(DAEMON_STATE_FILE));
        let _ = fs::remove_dir_all(cache_dir.join(DAEMON_LOCK_DIR));
        return Ok(());
    }

    // Create stop signal
    crate::signal::create_stop_signal(&cache_dir).context("Failed to create stop signal")?;
    println!("Stop signal sent to daemon (PID: {})", state.pid);

    // Wait up to 10 seconds for the daemon to exit
    println!("Waiting for daemon to stop...");
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(100));
        if !is_pid_running(state.pid) {
            println!("Daemon stopped successfully");
            let _ = fs::remove_file(cache_dir.join(DAEMON_STATE_FILE));
            return Ok(());
        }
    }

    // Daemon didn't stop in time
    bail!(
        "Daemon did not stop within 10 seconds. PID: {}. You may need to kill it manually with `kill -9 {}`",
        state.pid,
        state.pid
    );
}

/// Show daemon status.
pub fn status(resolved: &Resolved) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");

    match get_daemon_state(&cache_dir)? {
        Some(state) => {
            if is_pid_running(state.pid) {
                println!("Daemon is running");
                println!("  PID: {}", state.pid);
                println!("  Started: {}", state.started_at);
                println!("  Command: {}", state.command);
            } else {
                println!("Daemon is not running (stale state file detected)");
                println!("  Last PID: {}", state.pid);
                println!("  Last started: {}", state.started_at);
                // Clean up stale state
                let _ = fs::remove_file(cache_dir.join(DAEMON_STATE_FILE));
                let _ = fs::remove_dir_all(cache_dir.join(DAEMON_LOCK_DIR));
            }
        }
        None => {
            println!("Daemon is not running");
        }
    }

    Ok(())
}

/// Internal: Run the daemon serve loop.
/// This should not be called directly by users.
pub fn serve(resolved: &Resolved, args: DaemonServeArgs) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let daemon_lock_dir = cache_dir.join(DAEMON_LOCK_DIR);

    // Acquire the daemon lock
    let _lock = acquire_dir_lock(&daemon_lock_dir, "daemon", false)
        .context("Failed to acquire daemon lock")?;

    // Write daemon state
    let state = DaemonState {
        version: 1,
        pid: std::process::id(),
        started_at: crate::timeutil::now_utc_rfc3339()?,
        repo_root: resolved.repo_root.display().to_string(),
        command: std::env::args().collect::<Vec<_>>().join(" "),
    };
    write_daemon_state(&cache_dir, &state)?;

    log::info!(
        "Daemon started (PID: {}, empty_poll={}ms, wait_poll={}ms)",
        state.pid,
        args.empty_poll_ms,
        args.wait_poll_ms
    );

    // Run the continuous execution loop
    let result = crate::commands::run::run_loop(
        resolved,
        crate::commands::run::RunLoopOptions {
            max_tasks: 0, // No limit in daemon mode
            agent_overrides: crate::agent::AgentOverrides::default(),
            force: true, // Force mode for unattended operation
            auto_resume: false,
            starting_completed: 0,
            non_interactive: true,
            parallel_workers: None,
            wait_when_blocked: true,
            wait_poll_ms: args.wait_poll_ms,
            wait_timeout_seconds: 0, // No timeout in daemon mode
            notify_when_unblocked: args.notify_when_unblocked,
            wait_when_empty: true,
            empty_poll_ms: args.empty_poll_ms,
        },
    );

    // Clean up state on exit
    log::info!("Daemon shutting down");
    let _ = fs::remove_file(cache_dir.join(DAEMON_STATE_FILE));

    result
}

/// Read daemon state from disk.
fn get_daemon_state(cache_dir: &Path) -> Result<Option<DaemonState>> {
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
fn write_daemon_state(cache_dir: &Path, state: &DaemonState) -> Result<()> {
    let path = cache_dir.join(DAEMON_STATE_FILE);
    let content =
        serde_json::to_string_pretty(state).context("Failed to serialize daemon state")?;
    crate::fsutil::write_atomic(&path, content.as_bytes())
        .with_context(|| format!("Failed to write daemon state to {}", path.display()))?;
    Ok(())
}

/// Poll daemon state until it matches `pid` or a timeout elapses.
fn wait_for_daemon_state_pid(
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

/// Check if a PID is running.
fn is_pid_running(pid: u32) -> bool {
    lock::pid_is_running(pid).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
