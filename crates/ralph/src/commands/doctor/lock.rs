//! Lock directory health checks for the doctor command.
//!
//! Purpose:
//! - Lock directory health checks for the doctor command.
//!
//! Responsibilities:
//! - Inspect the canonical queue lock path using the shared run-time lock semantics.
//! - Surface live, stale, and metadata-broken queue locks through doctor results.
//! - Optionally remove only confirmed-stale queue locks when auto-fix is enabled.
//!
//! Not handled here:
//! - Active lock acquisition (see lock module)
//! - Queue content validation (see queue.rs)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Only confirmed dead-PID stale locks are auto-fixable here.
//! - Live, indeterminate, owner-missing, and unreadable-owner locks require operator review.
//! - Stale-lock auto-fix reuses `queue::acquire_queue_lock(..., force=true)` so removal stays
//!   aligned with runtime stale classification and remains safe to retry (no direct `remove_dir_all` bypass).

use crate::commands::doctor::types::{CheckResult, DoctorReport};
use crate::commands::run::{QueueLockCondition, inspect_queue_lock};
use crate::config;
use crate::lock::queue_lock_dir;
use anyhow::Context;
use std::path::Path;

pub(crate) fn check_lock_health(
    report: &mut DoctorReport,
    resolved: &config::Resolved,
    auto_fix: bool,
) {
    let Some(inspection) = inspect_queue_lock(&resolved.repo_root) else {
        report.add(CheckResult::success(
            "lock",
            "queue_lock_health",
            "queue lock is clear",
        ));
        return;
    };

    let fix_available = matches!(inspection.condition, QueueLockCondition::Stale);
    let suggested_fix = match inspection.condition {
        QueueLockCondition::Live => Some(
            "Wait for the owning Ralph process to finish, or inspect the current lock owner before retrying.",
        ),
        QueueLockCondition::Stale => Some(
            "Use --auto-fix to remove the confirmed stale queue lock, or clear it manually once you verify the recorded PID is dead.",
        ),
        QueueLockCondition::OwnerMissing | QueueLockCondition::OwnerUnreadable => Some(
            "Verify no other Ralph process is active, then clear the broken queue lock record explicitly.",
        ),
    };

    let mut result = CheckResult::error(
        "lock",
        "queue_lock_health",
        &inspection.blocking_state.message,
        fix_available,
        suggested_fix,
    )
    .with_blocking(inspection.blocking_state.clone());

    if auto_fix && fix_available {
        match remove_stale_queue_lock(&resolved.repo_root) {
            Ok(removed) => {
                log::info!("Removed stale queue lock: {}", removed);
                result = result.with_fix_applied(true);
            }
            Err(remove_err) => {
                log::error!("Failed to remove stale queue lock: {}", remove_err);
                result = result.with_fix_applied(false);
            }
        }
    }

    report.add(result);
}

fn remove_stale_queue_lock(repo_root: &Path) -> anyhow::Result<String> {
    let lock_dir = queue_lock_dir(repo_root);
    let _guard =
        crate::queue::acquire_queue_lock(repo_root, "doctor stale queue lock auto-fix", true)
            .with_context(|| format!("clear stale queue lock at {}", lock_dir.display()))?;
    Ok(lock_dir.display().to_string())
}
