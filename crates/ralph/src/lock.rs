//! Directory lock helpers for queue/task coordination.
//!
//! Responsibilities:
//! - Provide directory-based locks for queue/task operations.
//! - Record lock ownership metadata (PID, timestamp, command, label).
//! - Detect supervising processes and stale lock holders.
//! - Support shared task locks when a supervising process owns the lock.
//! - Provide tri-state PID liveness checks (Running, NotRunning, Indeterminate).
//!
//! Not handled here:
//! - Atomic writes or temp file cleanup (see `crate::fsutil`).
//! - Cross-machine locking or distributed coordination.
//! - Lock timeouts/backoff beyond the current retry/force logic.
//!
//! Invariants/assumptions:
//! - Callers hold `DirLock` for the entire critical section.
//! - The lock directory path is stable for the resource being protected.
//! - The "task" label is reserved for shared lock semantics.
//! - Labels are informational and should be trimmed before evaluation.
//! - Task lock sidecar files use unique names (owner_task_<pid>_<counter>) to prevent
//!   collisions when multiple task locks are acquired from the same process.
//! - Indeterminate PID liveness is treated conservatively as lock-owned to prevent
//!   concurrent supervisors and unsafe state cleanup.

use crate::constants::limits::MAX_RETRIES;
use crate::constants::timeouts::DELAYS_MS;
use crate::fsutil::sync_dir_best_effort;
use crate::timeutil;
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

/// Prefix for task owner sidecar files (shared lock mode).
pub(crate) const TASK_OWNER_PREFIX: &str = "owner_task_";

/// Per-process counter for generating unique task owner file names.
/// This ensures multiple task locks from the same process don't collide.
static TASK_OWNER_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Tri-state PID liveness result.
///
/// Used to distinguish between definitive running/not-running states and
/// indeterminate cases where we cannot determine the process status (e.g.,
/// permission errors, unsupported platforms).
///
/// Safety principle: Indeterminate liveness is treated conservatively as
/// lock-owned to prevent concurrent supervisors and unsafe state cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidLiveness {
    /// Process is definitely running.
    Running,
    /// Process is definitely not running (dead/zombie).
    NotRunning,
    /// Process status cannot be determined (permission error, unsupported platform).
    Indeterminate,
}

impl PidLiveness {
    /// Returns true if the process is definitely not running.
    ///
    /// Use this for stale detection: only treat a lock as stale when we have
    /// definitive evidence the owner is dead.
    pub fn is_definitely_not_running(self) -> bool {
        matches!(self, Self::NotRunning)
    }

    /// Returns true if the process is running or status is indeterminate.
    ///
    /// Use this for lock ownership checks: preserve locks when we cannot
    /// definitively prove the owner is dead.
    pub fn is_running_or_indeterminate(self) -> bool {
        matches!(self, Self::Running | Self::Indeterminate)
    }
}

/// Check PID liveness with tri-state result.
///
/// Wraps `pid_is_running` to provide a more expressive result type
/// that distinguishes between running, not-running, and indeterminate states.
pub fn pid_liveness(pid: u32) -> PidLiveness {
    match pid_is_running(pid) {
        Some(true) => PidLiveness::Running,
        Some(false) => PidLiveness::NotRunning,
        None => PidLiveness::Indeterminate,
    }
}

#[derive(Debug)]
pub struct DirLock {
    lock_dir: PathBuf,
    owner_path: PathBuf,
}

impl Drop for DirLock {
    fn drop(&mut self) {
        // Attempt to clean up the lock with retry logic to handle race conditions.
        // This prevents orphaned lock directories when another process creates
        // a file in the lock directory between removing the owner file and removing the directory.
        if let Err(e) = cleanup_lock_dir(&self.lock_dir, &self.owner_path, false) {
            log::warn!("Failed to clean up lock directory after retries: {}", e);
        }
    }
}

/// Check if a filename is a task owner sidecar file.
///
/// Task owner files follow the pattern: owner_task_<pid>_<counter>
/// or the legacy pattern: owner_task_<pid>
pub(crate) fn is_task_owner_file(name: &str) -> bool {
    name.starts_with(TASK_OWNER_PREFIX)
}

/// Check if the current owner is a task sidecar file.
fn is_task_sidecar_owner(owner_path: &Path) -> bool {
    owner_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(is_task_owner_file)
        .unwrap_or(false)
}

/// Check if any owner files remain in the lock directory after removing the current owner.
///
/// Returns true if there are other owner files present (either "owner" or other task sidecars).
fn has_other_owner_files(lock_dir: &Path, removed_owner_path: &Path) -> Result<bool> {
    if !lock_dir.exists() {
        return Ok(false);
    }

    for entry in fs::read_dir(lock_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip non-files
        if !path.is_file() {
            continue;
        }

        // Skip the owner file we just removed
        if path == removed_owner_path {
            continue;
        }

        let file_name = entry.file_name();
        let name = file_name.to_str().unwrap_or("");

        // Check if this is an owner file (either "owner" or task sidecar)
        if name == "owner" || is_task_owner_file(name) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Cleanup the lock directory with retry logic and exponential backoff.
///
/// This function handles the race condition where another process may create
/// a file in the lock directory between removing the owner file and removing
/// the directory itself.
///
/// For task sidecar owners, it also checks if other owner files remain in the
/// directory and skips directory removal to avoid interfering with supervising
/// locks or other task sidecars.
///
/// # Arguments
/// * `lock_dir` - The lock directory path
/// * `owner_path` - The owner file path to remove first
/// * `force` - If true, use `remove_dir_all` for aggressive cleanup on final attempt
///
/// # Returns
/// * `Ok(())` if cleanup succeeded
/// * `Err` if cleanup failed after all retries
fn cleanup_lock_dir(lock_dir: &Path, owner_path: &Path, force: bool) -> Result<()> {
    // Determine if this is a task sidecar owner before removing it
    let is_task_sidecar = is_task_sidecar_owner(owner_path);

    // First, remove the owner file
    if let Err(e) = fs::remove_file(owner_path) {
        // If the file doesn't exist, that's fine - continue to try cleaning the directory
        if e.kind() != std::io::ErrorKind::NotFound {
            log::debug!(
                "Failed to remove owner file {}: {}",
                owner_path.display(),
                e
            );
        }
    }

    // For task sidecars, check if other owners remain and skip directory removal if so.
    // This prevents a task lock from removing the lock directory while a supervisor
    // or other task sidecars still hold the lock.
    if is_task_sidecar {
        match has_other_owner_files(lock_dir, owner_path) {
            Ok(true) => {
                log::debug!(
                    "Skipping directory cleanup for task lock {} - other owners remain",
                    lock_dir.display()
                );
                return Ok(());
            }
            Ok(false) => {
                // No other owners, proceed with directory removal
            }
            Err(e) => {
                log::debug!(
                    "Failed to check for other owner files in {}: {}. Proceeding with cleanup...",
                    lock_dir.display(),
                    e
                );
            }
        }
    }

    // Attempt to remove the directory with retries
    for attempt in 0..MAX_RETRIES {
        // Try removing the directory (only succeeds if empty)
        match fs::remove_dir(lock_dir) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // If directory doesn't exist, we're done
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Ok(());
                }

                // On final attempt, try force cleanup if requested
                if attempt == MAX_RETRIES - 1 && force {
                    log::debug!(
                        "Attempting force cleanup of lock directory {}",
                        lock_dir.display()
                    );
                    match fs::remove_dir_all(lock_dir) {
                        Ok(()) => return Ok(()),
                        Err(force_err) => {
                            return Err(anyhow::anyhow!(
                                "Failed to force remove lock directory {}: {} (original error: {})",
                                lock_dir.display(),
                                force_err,
                                e
                            ));
                        }
                    }
                }

                // Log warning and retry with backoff
                log::warn!(
                    "Lock directory cleanup attempt {}/{} failed for {}: {}. Retrying...",
                    attempt + 1,
                    MAX_RETRIES,
                    lock_dir.display(),
                    e
                );

                if attempt < MAX_RETRIES - 1 {
                    thread::sleep(Duration::from_millis(DELAYS_MS[attempt as usize]));
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to remove lock directory {} after {} attempts",
        lock_dir.display(),
        MAX_RETRIES
    ))
}

/// Lock owner metadata parsed from the owner file.
#[derive(Debug, Clone)]
pub struct LockOwner {
    /// Process ID that holds the lock.
    pub pid: u32,
    /// ISO 8601 timestamp when the lock was acquired.
    pub started_at: String,
    /// Command line of the process that acquired the lock.
    pub command: String,
    /// Label describing the lock purpose (e.g., "run one", "task").
    pub label: String,
}

impl LockOwner {
    fn render(&self) -> String {
        format!(
            "pid: {}\nstarted_at: {}\ncommand: {}\nlabel: {}\n",
            self.pid, self.started_at, self.command, self.label
        )
    }
}

pub fn queue_lock_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph").join("lock")
}

fn is_supervising_label(label: &str) -> bool {
    matches!(label, "run one" | "run loop")
}

/// Check if the queue lock is currently held by a supervising process
/// (run one or run loop), which means the caller is running under
/// ralph's supervision and should not attempt to acquire the lock.
pub fn is_supervising_process(lock_dir: &Path) -> Result<bool> {
    let owner_path = lock_dir.join("owner");

    let raw = match fs::read_to_string(&owner_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(anyhow!(err))
                .with_context(|| format!("read lock owner {}", owner_path.display()));
        }
    };

    let owner = match parse_lock_owner(&raw) {
        Some(owner) => owner,
        None => return Ok(false),
    };

    Ok(is_supervising_label(&owner.label))
}

/// Check if the current process is running under ralph's supervision.
/// This returns true only if:
/// 1. A supervising process holds the lock, AND
/// 2. The current process is a descendant of that supervising process (same process group)
///
/// This distinguishes between:
/// - An agent running inside a supervised session (should use completion signals)
/// - A user manually running commands while a supervisor is active (should use direct completion)
pub fn is_current_process_supervised(lock_dir: &Path) -> Result<bool> {
    let owner_path = lock_dir.join("owner");

    let raw = match fs::read_to_string(&owner_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(anyhow!(err))
                .with_context(|| format!("read lock owner {}", owner_path.display()));
        }
    };

    let owner = match parse_lock_owner(&raw) {
        Some(owner) => owner,
        None => return Ok(false),
    };

    // Check if the lock holder is a supervising process
    if !is_supervising_label(&owner.label) {
        return Ok(false);
    }

    // Check if the current process is the same as or a descendant of the supervisor
    // On Unix, we can check if the current process group matches the supervisor's process group
    #[cfg(unix)]
    {
        let current_pid = std::process::id();
        let supervisor_pid = owner.pid;

        // If the current process IS the supervisor, it's not "supervised"
        if current_pid == supervisor_pid {
            return Ok(false);
        }

        // Check if the supervisor is still running and is our ancestor
        // by comparing process groups
        let current_pgid = unsafe { libc::getpgrp() };
        let supervisor_pgid = unsafe { libc::getpgid(supervisor_pid as i32) };

        if supervisor_pgid < 0 {
            // Error getting supervisor's process group, assume not supervised
            return Ok(false);
        }

        // If we're in the same process group as the supervisor, we're supervised
        Ok(current_pgid == supervisor_pgid)
    }

    #[cfg(not(unix))]
    {
        let current_pid = std::process::id();
        let liveness = pid_liveness(owner.pid);
        Ok(current_pid != owner.pid && liveness.is_running_or_indeterminate())
    }
}

pub fn acquire_dir_lock(lock_dir: &Path, label: &str, force: bool) -> Result<DirLock> {
    log::debug!(
        "acquiring dir lock: {} (label: {})",
        lock_dir.display(),
        label
    );
    if let Some(parent) = lock_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create lock parent {}", parent.display()))?;
    }

    let trimmed_label = label.trim();
    let is_task_label = trimmed_label == "task";

    match fs::create_dir(lock_dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            let mut owner_unreadable = false;
            let owner = match read_lock_owner(lock_dir) {
                Ok(owner) => owner,
                Err(_) => {
                    owner_unreadable = true;
                    None
                }
            };

            let is_stale = owner
                .as_ref()
                .is_some_and(|o| pid_liveness(o.pid).is_definitely_not_running());

            if force && is_stale {
                let _ = fs::remove_dir_all(lock_dir);
                // Retry once
                return acquire_dir_lock(lock_dir, label, false);
            }

            // Shared lock mode: "task" label can coexist with supervising lock
            if is_task_label
                && owner
                    .as_ref()
                    .is_some_and(|o| is_supervising_label(&o.label))
            {
                // Proceed to create sidecar owner file below
            } else {
                let msg = format_lock_error(lock_dir, owner.as_ref(), is_stale, owner_unreadable);
                return Err(anyhow!(msg));
            }
        }
        Err(err) => {
            return Err(anyhow!(err))
                .with_context(|| format!("create lock dir {}", lock_dir.display()));
        }
    }

    let effective_label = if trimmed_label.is_empty() {
        "unspecified"
    } else {
        trimmed_label
    };
    let owner = LockOwner {
        pid: std::process::id(),
        started_at: timeutil::now_utc_rfc3339()?,
        command: command_line(),
        label: effective_label.to_string(),
    };

    // For "task" label in shared lock mode, create sidecar owner file with unique name.
    // Use a per-process counter to ensure uniqueness when multiple task locks
    // are acquired from the same process.
    let owner_path = if is_task_label && lock_dir.exists() {
        let counter = TASK_OWNER_COUNTER.fetch_add(1, Ordering::SeqCst);
        lock_dir.join(format!("owner_task_{}_{}", std::process::id(), counter))
    } else {
        lock_dir.join("owner")
    };

    if let Err(err) = write_lock_owner(&owner_path, &owner) {
        let _ = fs::remove_file(&owner_path);

        // Best-effort cleanup: if the lock directory is empty, remove it.
        // This prevents task lock attempts from leaving an empty `.ralph/lock` behind on errors.
        let _ = fs::remove_dir(lock_dir);

        return Err(err);
    }

    Ok(DirLock {
        lock_dir: lock_dir.to_path_buf(),
        owner_path,
    })
}

fn format_lock_error(
    lock_dir: &Path,
    owner: Option<&LockOwner>,
    is_stale: bool,
    owner_unreadable: bool,
) -> String {
    let mut msg = format!("Queue lock already held at: {}", lock_dir.display());
    if is_stale {
        msg.push_str(" (STALE PID)");
    }
    if owner_unreadable {
        msg.push_str(" (owner metadata unreadable)");
    }

    msg.push_str("\n\nLock Holder:");
    if let Some(owner) = owner {
        msg.push_str(&format!(
            "\n  PID: {}\n  Label: {}\n  Started At: {}\n  Command: {}",
            owner.pid, owner.label, owner.started_at, owner.command
        ));
    } else {
        msg.push_str("\n  (owner metadata missing)");
    }

    msg.push_str("\n\nSuggested Action:");
    if is_stale {
        msg.push_str(&format!(
            "\n  The process that held this lock is no longer running.\n  Use --force to automatically clear it, or use the built-in unlock command (unsafe if another ralph is running):\n  ralph queue unlock\n  Or remove the directory manually:\n  rm -rf {}",
            lock_dir.display()
        ));
    } else {
        msg.push_str(&format!(
            "\n  If you are sure no other ralph process is running, use the built-in unlock command:\n  ralph queue unlock\n  Or remove the lock directory manually:\n  rm -rf {}",
            lock_dir.display()
        ));
    }
    msg
}

fn write_lock_owner(owner_path: &Path, owner: &LockOwner) -> Result<()> {
    log::debug!("writing lock owner: {}", owner_path.display());
    let mut file = fs::File::create(owner_path)
        .with_context(|| format!("create lock owner {}", owner_path.display()))?;
    file.write_all(owner.render().as_bytes())
        .context("write lock owner")?;
    file.flush().context("flush lock owner")?;
    file.sync_all().context("sync lock owner")?;
    if let Some(parent) = owner_path.parent() {
        sync_dir_best_effort(parent);
    }
    Ok(())
}

/// Read the lock owner metadata from the lock directory.
///
/// Returns `Ok(None)` if no owner file exists, `Ok(Some(LockOwner))` if the
/// owner file exists and is valid, or an error if the file cannot be read.
pub fn read_lock_owner(lock_dir: &Path) -> Result<Option<LockOwner>> {
    let owner_path = lock_dir.join("owner");
    let raw = match fs::read_to_string(&owner_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(anyhow!(err))
                .with_context(|| format!("read lock owner {}", owner_path.display()));
        }
    };
    Ok(parse_lock_owner(&raw))
}

fn parse_lock_owner(raw: &str) -> Option<LockOwner> {
    let mut pid = None;
    let mut started_at = None;
    let mut command = None;
    let mut label = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let value = value.trim().to_string();
            match key.trim() {
                "pid" => {
                    pid = value
                        .parse::<u32>()
                        .inspect_err(|e| {
                            log::debug!("Lock file has invalid pid '{}': {}", value, e)
                        })
                        .ok()
                }
                "started_at" => started_at = Some(value),
                "command" => command = Some(value),
                "label" => label = Some(value),
                _ => {}
            }
        }
    }

    let pid = pid?;
    Some(LockOwner {
        pid,
        started_at: started_at.unwrap_or_else(|| "unknown".to_string()),
        command: command.unwrap_or_else(|| "unknown".to_string()),
        label: label.unwrap_or_else(|| "unknown".to_string()),
    })
}

/// Check if a process with the given PID is currently running.
///
/// Returns:
/// - `Some(true)` if the process exists
/// - `Some(false)` if the process definitely does not exist
/// - `None` if the status cannot be determined (platform unsupported or permission error)
pub fn pid_is_running(pid: u32) -> Option<bool> {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result == 0 {
            return Some(true);
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            return Some(false);
        }
        None
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER};
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
            if handle != 0 {
                // Process exists - close the handle and return true
                CloseHandle(handle);
                Some(true)
            } else {
                // OpenProcess failed - check why
                let err = windows_sys::Win32::Foundation::GetLastError();
                if err == ERROR_INVALID_PARAMETER {
                    // Invalid PID means process doesn't exist
                    Some(false)
                } else {
                    // Other error - can't determine status
                    None
                }
            }
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        None
    }
}

fn command_line() -> String {
    let args: Vec<String> = std::env::args().collect();
    let joined = args.join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that the current process PID is detected as running.
    /// This test works on both Unix and Windows platforms.
    #[test]
    fn test_pid_is_running_current_process() {
        let current_pid = std::process::id();
        let result = pid_is_running(current_pid);
        // Should be Some(true) on supported platforms
        assert_eq!(
            result,
            Some(true),
            "Current process should be detected as running"
        );
    }

    /// Test that a non-existent PID is detected as not running.
    /// Uses a very high PID that's extremely unlikely to exist.
    #[test]
    fn test_pid_is_running_nonexistent() {
        // Use the maximum possible PID value (0xFFFFFFFF on 32-bit systems)
        // This is extremely unlikely to be a real process
        let result = pid_is_running(0xFFFFFFFE);
        // Should be either Some(false) or None, never Some(true)
        assert_ne!(
            result,
            Some(true),
            "Non-existent PID should not return Some(true)"
        );
    }

    /// Test that PID 0 is handled correctly.
    /// On Unix, PID 0 is a special "swapper/sched" process that always exists.
    /// On Windows, PID 0 is the "System Idle Process" which is also always present.
    #[test]
    fn test_pid_is_running_system_idle() {
        let result = pid_is_running(0);
        // PID 0 should either be running or indeterminate, never explicitly "not running"
        // On most systems, PID 0 exists and is the idle process
        if result == Some(false) {
            panic!("PID 0 should not be reported as not running");
        }
    }

    /// Test that the task owner file detection works correctly.
    #[test]
    fn test_is_task_owner_file() {
        assert!(is_task_owner_file("owner_task_1234"));
        assert!(is_task_owner_file("owner_task_1234_0"));
        assert!(is_task_owner_file("owner_task_1234_42"));
        assert!(!is_task_owner_file("owner"));
        assert!(!is_task_owner_file("owner_other"));
        assert!(!is_task_owner_file("owner_task"));
        assert!(!is_task_owner_file(""));
        assert!(!is_task_owner_file("task_owner_1234"));
    }

    /// Test that PidLiveness helper methods work correctly.
    #[test]
    fn test_pid_liveness_helpers() {
        assert!(PidLiveness::NotRunning.is_definitely_not_running());
        assert!(!PidLiveness::Running.is_definitely_not_running());
        assert!(!PidLiveness::Indeterminate.is_definitely_not_running());

        assert!(PidLiveness::Running.is_running_or_indeterminate());
        assert!(PidLiveness::Indeterminate.is_running_or_indeterminate());
        assert!(!PidLiveness::NotRunning.is_running_or_indeterminate());
    }

    /// Test that pid_liveness wraps pid_is_running correctly.
    #[test]
    fn test_pid_liveness_wrapper() {
        let current_pid = std::process::id();
        assert_eq!(pid_liveness(current_pid), PidLiveness::Running);

        // High PID is unlikely to exist; should be NotRunning or Indeterminate
        let result = pid_liveness(0xFFFFFFFE);
        assert!(matches!(
            result,
            PidLiveness::NotRunning | PidLiveness::Indeterminate
        ));
    }

    /// Test that indeterminate liveness is treated conservatively as lock-owned.
    #[test]
    fn test_stale_lock_detection_is_conservative() {
        // Only NotRunning should be treated as stale
        assert!(!PidLiveness::Running.is_definitely_not_running());
        assert!(!PidLiveness::Indeterminate.is_definitely_not_running());
        assert!(PidLiveness::NotRunning.is_definitely_not_running());
    }
}
