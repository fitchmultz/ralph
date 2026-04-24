//! Lock cleanup helpers.
//!
//! Purpose:
//! - Lock cleanup helpers.
//!
//! Responsibilities:
//! - Remove owner files and lock directories with retry/backoff handling.
//! - Preserve shared task/supervisor lock semantics during cleanup.
//!
//! Not handled here:
//! - Lock acquisition decisions or stale-lock policy.
//! - PID liveness detection.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Task sidecars must not remove the lock directory while other owner files remain.

use super::owner::{OWNER_FILE_NAME, is_task_owner_file, is_task_sidecar_owner};
use crate::constants::limits::MAX_RETRIES;
use crate::constants::timeouts::DELAYS_MS;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

pub(crate) fn cleanup_lock_dir(lock_dir: &Path, owner_path: &Path, force: bool) -> Result<()> {
    let is_task_sidecar = is_task_sidecar_owner(owner_path);

    if let Err(error) = fs::remove_file(owner_path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        log::debug!(
            "Failed to remove owner file {}: {}",
            owner_path.display(),
            error
        );
    }

    if is_task_sidecar {
        match has_other_owner_files(lock_dir, owner_path) {
            Ok(true) => {
                log::debug!(
                    "Skipping directory cleanup for task lock {} - other owners remain",
                    lock_dir.display()
                );
                return Ok(());
            }
            Ok(false) => {}
            Err(error) => {
                log::debug!(
                    "Failed to check for other owner files in {}: {}. Proceeding with cleanup...",
                    lock_dir.display(),
                    error
                );
            }
        }
    }

    for attempt in 0..MAX_RETRIES {
        match fs::remove_dir(lock_dir) {
            Ok(()) => return Ok(()),
            Err(error) => {
                if error.kind() == std::io::ErrorKind::NotFound {
                    return Ok(());
                }

                if attempt == MAX_RETRIES - 1 && force {
                    match fs::remove_dir_all(lock_dir) {
                        Ok(()) => return Ok(()),
                        Err(force_error) => {
                            return Err(anyhow!(
                                "Failed to force remove lock directory {}: {} (original error: {})",
                                lock_dir.display(),
                                force_error,
                                error
                            ));
                        }
                    }
                }

                log::warn!(
                    "Lock directory cleanup attempt {}/{} failed for {}: {}. Retrying...",
                    attempt + 1,
                    MAX_RETRIES,
                    lock_dir.display(),
                    error
                );

                if attempt < MAX_RETRIES - 1 {
                    thread::sleep(Duration::from_millis(DELAYS_MS[attempt as usize]));
                }
            }
        }
    }

    Err(anyhow!(
        "Failed to remove lock directory {} after {} attempts",
        lock_dir.display(),
        MAX_RETRIES
    ))
}

fn has_other_owner_files(lock_dir: &Path, removed_owner_path: &Path) -> Result<bool> {
    if !lock_dir.exists() {
        return Ok(false);
    }

    for entry in fs::read_dir(lock_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path == removed_owner_path {
            continue;
        }

        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name == OWNER_FILE_NAME || is_task_owner_file(name) {
            return Ok(true);
        }
    }

    Ok(false)
}
