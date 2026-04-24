//! Daemon start command implementation.
//!
//! Purpose:
//! - Daemon start command implementation.
//!
//! Responsibilities:
//! - Start the daemon as a background process on Unix systems.
//! - Check for existing running daemon and handle stale state.
//! - Acquire daemon lock to prevent concurrent starts.
//! - Spawn the daemon process with proper stdio redirection.
//! - Wait for daemon state to confirm successful startup.
//!
//! Not handled here:
//! - Windows daemon management (returns error on non-Unix systems).
//! - Daemon stop/status/logs operations (handled in other modules).
//! - Signal handling or process lifecycle management after spawn.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Requires Unix platform; fails gracefully on Windows.
//! - Daemon state file is written by the spawned serve process.
//! - Uses DAEMON_LOCK_DIR for exclusive access during startup.

use crate::cli::daemon::DaemonStartArgs;
use crate::config::Resolved;
use crate::lock::{PidLiveness, acquire_dir_lock};
use anyhow::{Context, Result, bail};
use std::fs;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use super::{
    DAEMON_LOG_FILE_NAME, DAEMON_START_LOCK_DIR, clear_daemon_runtime_artifacts,
    daemon_pid_liveness, get_daemon_state, manual_daemon_cleanup_instructions,
    wait_for_daemon_ready,
};

/// Start the daemon as a background process.
pub fn start(resolved: &Resolved, args: DaemonStartArgs) -> Result<()> {
    #[cfg(unix)]
    {
        let cache_dir = resolved.repo_root.join(".ralph/cache");
        let daemon_start_lock_dir = cache_dir.join(DAEMON_START_LOCK_DIR);

        // Check if daemon is already running
        if let Some(state) = get_daemon_state(&cache_dir)? {
            match daemon_pid_liveness(state.pid) {
                PidLiveness::Running => {
                    bail!(
                        "Daemon is already running (PID: {}). Use `ralph daemon stop` to stop it.",
                        state.pid
                    );
                }
                PidLiveness::Indeterminate => {
                    bail!(
                        "Daemon PID {} liveness is indeterminate. \
                         Preserving state/lock to prevent concurrent supervisors. \
                         {}",
                        state.pid,
                        manual_daemon_cleanup_instructions(&cache_dir)
                    );
                }
                PidLiveness::NotRunning => {
                    log::warn!("Removing stale daemon runtime artifacts");
                    clear_daemon_runtime_artifacts(&cache_dir, false);
                }
            }
        }

        // Acquire a dedicated startup lock so the child can own the runtime daemon lock.
        let _lock = match acquire_dir_lock(&daemon_start_lock_dir, "daemon-start", false) {
            Ok(lock) => lock,
            Err(e) => {
                bail!(
                    "Failed to acquire daemon start lock: {}. Another daemon may be starting.",
                    e
                );
            }
        };

        // Build the serve command
        let exe = std::env::current_exe().context("Failed to get current executable path")?;
        let mut command = std::process::Command::new(&exe);
        command.current_dir(&resolved.repo_root);
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
        let log_file = std::fs::File::create(log_dir.join(DAEMON_LOG_FILE_NAME))
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
        // SAFETY: pre_exec runs between fork and exec in the child process.
        // setsid() creates a new session and detaches from controlling terminal.
        // This is async-signal-safe per POSIX and safe to call here.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        // Spawn the daemon process
        let mut child = command.spawn().context("Failed to spawn daemon process")?;
        let pid = child.id();

        if wait_for_daemon_ready(&cache_dir, pid, Duration::from_secs(2), &mut child)? {
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
