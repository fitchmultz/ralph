//! Lock directory health checks for the doctor command.
//!
//! Responsibilities:
//! - Detect orphaned lock directories
//! - Check owner file validity and PID liveness
//! - Remove stale locks when auto-fix is enabled
//!
//! Not handled here:
//! - Active lock acquisition (see lock module)
//! - Queue content validation (see queue.rs)
//!
//! Invariants/assumptions:
//! - Lock directories without valid owners are safe to remove
//! - PID liveness checks may be indeterminate on some platforms

use crate::commands::doctor::types::{CheckResult, DoctorReport};
use crate::config;
use crate::lock::{is_task_owner_file, pid_liveness, queue_lock_dir};
use std::fs;
use std::path::Path;

pub(crate) fn check_lock_health(
    report: &mut DoctorReport,
    resolved: &config::Resolved,
    auto_fix: bool,
) {
    match check_lock_directory_health(&resolved.repo_root) {
        Ok((orphaned_count, total_count)) => {
            if orphaned_count > 0 {
                let fix_available = true;
                let mut result = CheckResult::warning(
                    "lock",
                    "orphaned_locks",
                    &format!(
                        "found {} orphaned lock director{} (out of {} total)",
                        orphaned_count,
                        if orphaned_count == 1 { "y" } else { "ies" },
                        total_count
                    ),
                    fix_available,
                    Some("Use --auto-fix to remove orphaned lock directories"),
                );

                if auto_fix && fix_available {
                    match remove_orphaned_locks(&resolved.repo_root) {
                        Ok(removed_count) => {
                            log::info!("Removed {} orphaned lock directories", removed_count);
                            result = result.with_fix_applied(true);
                        }
                        Err(remove_err) => {
                            log::error!("Failed to remove orphaned locks: {}", remove_err);
                            result = result.with_fix_applied(false);
                        }
                    }
                }

                report.add(result);
            } else if total_count > 0 {
                report.add(CheckResult::success(
                    "lock",
                    "lock_health",
                    &format!(
                        "all {} lock director{} healthy",
                        total_count,
                        if total_count == 1 { "y" } else { "ies" }
                    ),
                ));
            } else {
                log::info!("no lock directories found");
            }
        }
        Err(e) => {
            report.add(CheckResult::warning(
                "lock",
                "lock_health",
                &format!("lock health check failed: {}", e),
                false,
                None,
            ));
        }
    }
}

/// Check the health of lock directories in .ralph/lock/
///
/// Returns a tuple of (orphaned_count, total_count) where:
/// - orphaned_count: Number of lock directories that appear to be orphaned
/// - total_count: Total number of lock directories found
pub(crate) fn check_lock_directory_health(repo_root: &Path) -> anyhow::Result<(usize, usize)> {
    let lock_dir = queue_lock_dir(repo_root);

    if !lock_dir.exists() {
        return Ok((0, 0));
    }

    let mut total_count = 0;
    let mut orphaned_count = 0;

    for entry in fs::read_dir(&lock_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only consider directories
        if !path.is_dir() {
            continue;
        }

        total_count += 1;

        // Check if this lock directory has a valid owner file
        let owner_path = path.join("owner");
        let has_valid_owner = if owner_path.exists() {
            // Check if the owner file has a valid, running PID
            match fs::read_to_string(&owner_path) {
                Ok(content) => {
                    // Parse PID from owner file
                    content
                        .lines()
                        .find(|line| line.starts_with("pid:"))
                        .and_then(|line| line.split(':').nth(1))
                        .and_then(|pid_str| pid_str.trim().parse::<u32>().ok())
                        .map(|pid| pid_liveness(pid).is_running_or_indeterminate())
                        .unwrap_or(true) // Assume running if we can't determine status
                }
                Err(_) => false,
            }
        } else {
            // Check for task owner files (shared locks)
            // Use the shared helper from lock module to detect task sidecar files
            fs::read_dir(&path)?.any(|e| {
                e.ok()
                    .map(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .map(is_task_owner_file)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
        };

        if !has_valid_owner {
            orphaned_count += 1;
            log::debug!(
                "Orphaned lock directory detected: {} (no valid owner)",
                path.display()
            );
        }
    }

    Ok((orphaned_count, total_count))
}

/// Remove orphaned lock directories.
///
/// Returns the number of directories removed.
pub(crate) fn remove_orphaned_locks(repo_root: &Path) -> anyhow::Result<usize> {
    let lock_dir = queue_lock_dir(repo_root);

    if !lock_dir.exists() {
        return Ok(0);
    }

    let mut removed_count = 0;

    for entry in fs::read_dir(&lock_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only consider directories
        if !path.is_dir() {
            continue;
        }

        // Check if this lock directory has a valid owner file
        let owner_path = path.join("owner");
        let has_valid_owner = if owner_path.exists() {
            match fs::read_to_string(&owner_path) {
                Ok(content) => content
                    .lines()
                    .find(|line| line.starts_with("pid:"))
                    .and_then(|line| line.split(':').nth(1))
                    .and_then(|pid_str| pid_str.trim().parse::<u32>().ok())
                    .map(|pid| pid_liveness(pid).is_running_or_indeterminate())
                    .unwrap_or(true),
                Err(_) => false,
            }
        } else {
            fs::read_dir(&path)?.any(|e| {
                e.ok()
                    .map(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .map(is_task_owner_file)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
        };

        if !has_valid_owner {
            log::info!("Removing orphaned lock directory: {}", path.display());
            fs::remove_dir_all(&path)?;
            removed_count += 1;
        }
    }

    // Try to clean up the lock directory itself if it's now empty
    if lock_dir.exists() {
        let is_empty = fs::read_dir(&lock_dir)?.next().is_none();
        if is_empty {
            fs::remove_dir(&lock_dir)?;
        }
    }

    Ok(removed_count)
}
