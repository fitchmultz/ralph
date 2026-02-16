//! Queue health checks and repair for the doctor command.
//!
//! Responsibilities:
//! - Validate queue file existence and format
//! - Check done archive integrity
//! - Apply automatic queue repairs when requested
//!
//! Not handled here:
//! - Queue modification outside of repair context
//! - Lock directory management (see lock.rs)
//!
//! Invariants/assumptions:
//! - Queue validation failures can be repaired via queue::repair module
//! - Repairs are idempotent and safe to retry

use crate::commands::doctor::types::{CheckResult, DoctorReport};
use crate::config;
use crate::queue;

pub(crate) fn check_queue(report: &mut DoctorReport, resolved: &config::Resolved, auto_fix: bool) {
    if !resolved.queue_path.exists() {
        report.add(CheckResult::error(
            "queue",
            "queue_exists",
            &format!("queue file missing at {}", resolved.queue_path.display()),
            false,
            Some("Run 'ralph init' to create a new queue"),
        ));
        return;
    }

    match queue::load_queue(&resolved.queue_path) {
        Ok(q) => {
            match queue::validate_queue(&q, &resolved.id_prefix, resolved.id_width) {
                Ok(_) => {
                    report.add(CheckResult::success(
                        "queue",
                        "queue_valid",
                        &format!("queue valid ({} tasks)", q.tasks.len()),
                    ));
                }
                Err(e) => {
                    // Queue validation failed - offer repair as auto-fix
                    let fix_available = true;

                    if auto_fix && fix_available {
                        match apply_queue_repair(resolved) {
                            Ok(repair_report) => {
                                log::info!(
                                    "Queue repair applied: {} tasks fixed, {} timestamps fixed, {} IDs remapped",
                                    repair_report.fixed_tasks,
                                    repair_report.fixed_timestamps,
                                    repair_report.remapped_ids.len()
                                );

                                // Re-validate the queue after repair
                                match queue::load_queue(&resolved.queue_path) {
                                    Ok(repaired_q) => {
                                        match queue::validate_queue(
                                            &repaired_q,
                                            &resolved.id_prefix,
                                            resolved.id_width,
                                        ) {
                                            Ok(_) => {
                                                // Repair succeeded and queue is now valid
                                                report.add(CheckResult::success(
                                                    "queue",
                                                    "queue_valid",
                                                    &format!(
                                                        "queue valid after repair ({} tasks)",
                                                        repaired_q.tasks.len()
                                                    ),
                                                ));
                                            }
                                            Err(reval_err) => {
                                                // Repair was applied but validation still fails
                                                report.add(
                                                    CheckResult::error(
                                                        "queue",
                                                        "queue_valid",
                                                        &format!(
                                                            "queue validation failed: {}",
                                                            reval_err
                                                        ),
                                                        false,
                                                        Some("Manual repair required"),
                                                    )
                                                    .with_fix_applied(false),
                                                );
                                            }
                                        }
                                    }
                                    Err(load_err) => {
                                        report.add(
                                            CheckResult::error(
                                                "queue",
                                                "queue_load",
                                                &format!("failed to load queue after repair: {}", load_err),
                                                false,
                                                Some("Check queue file format or restore from backup"),
                                            )
                                            .with_fix_applied(false),
                                        );
                                    }
                                }
                            }
                            Err(repair_err) => {
                                log::error!("Failed to repair queue: {}", repair_err);
                                report.add(
                                    CheckResult::error(
                                        "queue",
                                        "queue_valid",
                                        &format!("queue validation failed: {}", e),
                                        fix_available,
                                        Some("Run 'ralph queue repair' to repair"),
                                    )
                                    .with_fix_applied(false),
                                );
                            }
                        }
                    } else {
                        // No auto-fix, report the error
                        report.add(CheckResult::error(
                            "queue",
                            "queue_valid",
                            &format!("queue validation failed: {}", e),
                            fix_available,
                            Some("Run 'ralph queue repair' or use --auto-fix to repair automatically"),
                        ));
                    }
                }
            }
        }
        Err(e) => {
            report.add(CheckResult::error(
                "queue",
                "queue_load",
                &format!("failed to load queue: {}", e),
                false,
                Some("Check queue file format or restore from backup"),
            ));
        }
    }
}

pub(crate) fn check_done_archive(report: &mut DoctorReport, resolved: &config::Resolved) {
    if !resolved.done_path.exists() {
        log::info!("done archive missing (optional)");
        return;
    }

    match queue::load_queue(&resolved.done_path) {
        Ok(d) => match queue::validate_queue(&d, &resolved.id_prefix, resolved.id_width) {
            Ok(_) => {
                report.add(CheckResult::success(
                    "queue",
                    "done_archive_valid",
                    &format!("done archive valid ({} tasks)", d.tasks.len()),
                ));
            }
            Err(e) => {
                report.add(CheckResult::error(
                    "queue",
                    "done_archive_valid",
                    &format!("done archive validation failed: {}", e),
                    false,
                    Some("Run 'ralph queue repair' to repair the done archive"),
                ));
            }
        },
        Err(e) => {
            report.add(CheckResult::error(
                "queue",
                "done_archive_load",
                &format!("failed to load done archive: {}", e),
                false,
                Some("Check done file format or restore from backup"),
            ));
        }
    }
}

/// Apply queue repair for auto-fix.
pub(crate) fn apply_queue_repair(
    resolved: &config::Resolved,
) -> anyhow::Result<queue::repair::RepairReport> {
    queue::repair::repair_queue(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
        false, // not dry run
    )
}
