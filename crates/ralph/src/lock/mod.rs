//! Directory lock facade for queue and daemon coordination.
//!
//! Purpose:
//! - Directory lock facade for queue and daemon coordination.
//!
//! Responsibilities:
//! - Expose lock acquisition, owner metadata, stale-lock handling, and PID liveness helpers.
//! - Keep concurrency-critical concerns split into focused submodules.
//!
//! Not handled here:
//! - Queue mutation or config validation.
//! - Cross-machine or distributed locking.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Callers hold `DirLock` for the full critical section.
//! - The lock directory path is stable for the resource being protected.
//! - The `task` label remains reserved for shared supervisor/task lock semantics.

mod acquisition;
mod cleanup;
mod owner;
mod pid;
mod stale;

use std::path::PathBuf;

pub use acquisition::{acquire_dir_lock, is_supervising_process, queue_lock_dir};
pub use owner::{LockOwner, TASK_OWNER_PREFIX, is_task_owner_file, read_lock_owner};
pub use pid::{PidLiveness, pid_is_running, pid_liveness};
pub(crate) use stale::{LockStaleness, classify_lock_owner};

/// Held directory lock guard.
#[derive(Debug)]
pub struct DirLock {
    lock_dir: PathBuf,
    owner_path: PathBuf,
}

impl Drop for DirLock {
    fn drop(&mut self) {
        if let Err(error) = cleanup::cleanup_lock_dir(&self.lock_dir, &self.owner_path, false) {
            log::warn!("Failed to clean up lock directory after retries: {}", error);
        }
    }
}

#[cfg(test)]
mod tests;
