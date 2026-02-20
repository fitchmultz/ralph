//! Daemon start command implementation.
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
    DAEMON_LOCK_DIR, DAEMON_LOG_FILE_NAME, DAEMON_STATE_FILE, daemon_pid_liveness,
    get_daemon_state, manual_daemon_cleanup_instructions, wait_for_daemon_state_pid,
};

/// Start the daemon as a background process.
pub fn start(resolved: &Resolved, args: DaemonStartArgs) -> Result<()> {
    #[cfg(unix)]
    {
        let cache_dir = resolved.repo_root.join(".ralph/cache");
        let daemon_lock_dir = cache_dir.join(DAEMON_LOCK_DIR);

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
                    log::warn!("Removing stale daemon state file");
                    let state_path = cache_dir.join(DAEMON_STATE_FILE);
                    if let Err(e) = fs::remove_file(&state_path) {
                        log::debug!(
                            "Failed to remove stale daemon state file {}: {}",
                            state_path.display(),
                            e
                        );
                    }
                }
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
