//! Queue lock helpers for run command.
//!
//! Purpose:
//! - Queue lock helpers for run command.
//!
//! Responsibilities:
//! - Clear stale queue locks when resuming a session.
//! - Detect queue lock contention errors that should not be retried.
//! - Build operator-facing lock-blocked state from current lock metadata.
//!
//! Not handled here:
//! - Lock acquisition logic (see `crate::queue`).
//! - Session management (see `crate::session`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Stale lock cleanup only clears locks whose owner PID is confirmed dead.
//! - Queue lock errors are non-retriable to prevent infinite loops.

use anyhow::Result;
use std::path::Path;

use crate::contracts::{BlockingReason, BlockingState, BlockingStatus};
use crate::lock::{classify_lock_owner, queue_lock_dir, read_lock_owner};

const QUEUE_LOCK_ALREADY_HELD_PREFIX: &str = "Queue lock already held at:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueueLockCondition {
    Live,
    Stale,
    OwnerMissing,
    OwnerUnreadable,
}

#[derive(Debug, Clone)]
pub(crate) struct QueueLockInspection {
    pub(crate) condition: QueueLockCondition,
    pub(crate) blocking_state: BlockingState,
}

impl QueueLockInspection {
    pub(crate) fn is_stale_or_unclear(&self) -> bool {
        !matches!(self.condition, QueueLockCondition::Live)
    }
}

/// Clear stale queue lock when resuming a session.
///
/// This helper is called during resume to preemptively clean up stale locks
/// left behind by a crashed or killed ralph process. Normal lock acquisition
/// also auto-clears confirmed-dead PID locks; this remains a resume-specific
/// preflight so resume decisions see a clean queue-lock state.
///
/// Returns Ok(()) if no lock exists, lock was cleared, or lock is held by
/// a live process/unreadable metadata (those cases are handled later during
/// normal acquisition with a single actionable error).
pub fn clear_stale_queue_lock_for_resume(repo_root: &Path) -> Result<()> {
    let lock_dir = queue_lock_dir(repo_root);
    if !lock_dir.exists() {
        return Ok(());
    }

    // We acquire+drop immediately: this performs stale cleanup without holding the lock.
    let lock = match crate::queue::acquire_queue_lock(repo_root, "run loop resume", true) {
        Ok(lock) => lock,
        Err(err) => {
            // If the lock is held by a live process, or the owner metadata is missing/unreadable,
            // we cannot safely clear it here. Let normal acquisition report the actionable error.
            if is_queue_lock_already_held_error(&err) {
                return Ok(());
            }
            return Err(err);
        }
    };
    drop(lock);
    Ok(())
}

/// Check if an error is a "Queue lock already held" error.
///
/// This is used to detect lock contention errors that should not be retried
/// in the run loop, preventing the 50-failure abort loop.
pub fn is_queue_lock_already_held_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .to_string()
            .starts_with(QUEUE_LOCK_ALREADY_HELD_PREFIX)
    })
}

pub(crate) fn inspect_queue_lock(repo_root: &Path) -> Option<QueueLockInspection> {
    let lock_dir = queue_lock_dir(repo_root);
    if !lock_dir.exists() {
        return None;
    }

    let lock_path = Some(lock_dir.display().to_string());
    match read_lock_owner(&lock_dir) {
        Ok(Some(owner)) => {
            let owner_label = Some(owner.label.clone());
            let owner_pid = Some(owner.pid);
            let staleness = classify_lock_owner(&owner);
            if staleness.is_stale() {
                Some(QueueLockInspection {
                    condition: QueueLockCondition::Stale,
                    blocking_state: BlockingState::new(
                        BlockingStatus::Stalled,
                        BlockingReason::LockBlocked {
                            lock_path,
                            owner: owner_label,
                            owner_pid,
                        },
                        None,
                        "Ralph is stalled on a stale queue lock.",
                        format!(
                            "The queue lock at {} was left behind by Ralph process {} (pid {}). Ralph will auto-clear this verified stale lock on the next queue-lock acquisition.",
                            lock_dir.display(),
                            owner.label,
                            owner.pid
                        ),
                    )
                    .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback()),
                })
            } else {
                let mut blocking_state =
                    BlockingState::lock_blocked(lock_path, owner_label, owner_pid);
                if let Some(note) = staleness.advisory_note() {
                    blocking_state.detail.push(' ');
                    blocking_state.detail.push_str(note.trim());
                }
                blocking_state = blocking_state
                    .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback());
                Some(QueueLockInspection {
                    condition: QueueLockCondition::Live,
                    blocking_state,
                })
            }
        }
        Ok(None) => Some(QueueLockInspection {
            condition: QueueLockCondition::OwnerMissing,
            blocking_state: BlockingState::new(
                BlockingStatus::Stalled,
                BlockingReason::LockBlocked {
                    lock_path,
                    owner: None,
                    owner_pid: None,
                },
                None,
                "Ralph is stalled on queue lock metadata cleanup.",
                format!(
                    "The queue lock at {} exists, but its owner metadata is missing. Confirm no other Ralph process is running, then clear the lock before resuming execution.",
                    lock_dir.display()
                ),
            )
            .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback()),
        }),
        Err(_) => Some(QueueLockInspection {
            condition: QueueLockCondition::OwnerUnreadable,
            blocking_state: BlockingState::new(
                BlockingStatus::Stalled,
                BlockingReason::LockBlocked {
                    lock_path,
                    owner: None,
                    owner_pid: None,
                },
                None,
                "Ralph is stalled on unreadable queue lock metadata.",
                format!(
                    "The queue lock at {} exists, but Ralph could not read its owner metadata. Confirm no other Ralph process is running, then clear the lock before resuming execution.",
                    lock_dir.display()
                ),
            )
            .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback()),
        }),
    }
}

pub fn queue_lock_blocking_state(repo_root: &Path, err: &anyhow::Error) -> Option<BlockingState> {
    if !is_queue_lock_already_held_error(err) {
        return None;
    }

    inspect_queue_lock(repo_root).map(|inspection| inspection.blocking_state)
}
