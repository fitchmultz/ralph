//! Archive operations for queue tasks.
//!
//! Purpose:
//! - Archive operations for queue tasks.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::backfill_terminal_completed_at;
use crate::contracts::{QueueFile, TaskStatus};
use crate::queue::{load_queue, load_queue_or_default, save_queue, validation};
use crate::timeutil;
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ArchiveReport {
    pub moved_ids: Vec<String>,
}

/// Archive terminal tasks (Done/Rejected) that are at least `after_days` old.
///
/// - If `after_days == 0`: delegates to `archive_terminal_tasks_in_memory` (immediate).
/// - If `after_days > 0`: only moves tasks where `completed_at` is at least `after_days` old.
///   Tasks with missing/invalid `completed_at` are NOT moved (safety-first).
pub fn archive_terminal_tasks_older_than_days_in_memory(
    active: &mut QueueFile,
    done: &mut QueueFile,
    now_rfc3339: &str,
    after_days: u32,
) -> Result<ArchiveReport> {
    if after_days == 0 {
        return archive_terminal_tasks_in_memory(active, done, now_rfc3339);
    }

    let now = timeutil::parse_rfc3339(now_rfc3339)?;
    let cutoff = now - time::Duration::days(after_days as i64);

    let mut moved_ids = Vec::new();
    let mut remaining = Vec::with_capacity(active.tasks.len());

    for mut task in active.tasks.drain(..) {
        if matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
            let eligible = task
                .completed_at
                .as_deref()
                .and_then(timeutil::parse_rfc3339_opt)
                .filter(|dt| dt.offset() == time::UtcOffset::UTC)
                .is_some_and(|dt| dt <= cutoff);

            if eligible {
                task.updated_at = Some(now_rfc3339.to_string());
                moved_ids.push(task.id.trim().to_string());
                done.tasks.push(task);
            } else {
                remaining.push(task);
            }
        } else {
            remaining.push(task);
        }
    }

    active.tasks = remaining;
    Ok(ArchiveReport { moved_ids })
}

/// Conditionally archive terminal tasks based on optional days config.
///
/// - `None`: returns empty report (disabled)
/// - `Some(days)`: delegates to `archive_terminal_tasks_older_than_days_in_memory`
pub fn maybe_archive_terminal_tasks_in_memory(
    active: &mut QueueFile,
    done: &mut QueueFile,
    now_rfc3339: &str,
    after_days: Option<u32>,
) -> Result<ArchiveReport> {
    match after_days {
        None => Ok(ArchiveReport {
            moved_ids: Vec::new(),
        }),
        Some(days) => {
            archive_terminal_tasks_older_than_days_in_memory(active, done, now_rfc3339, days)
        }
    }
}

/// Archive terminal tasks that are at least `after_days` old (disk-based).
///
/// This loads both queue files, performs the archive operation, validates,
/// and saves back to disk. Also backfills missing completed_at timestamps
/// in the done file.
pub fn archive_terminal_tasks_older_than_days(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
    after_days: u32,
) -> Result<ArchiveReport> {
    let mut active = load_queue(queue_path)?;
    let mut done = load_queue_or_default(done_path)?;

    let now = timeutil::now_utc_rfc3339()?;
    let report =
        archive_terminal_tasks_older_than_days_in_memory(&mut active, &mut done, &now, after_days)?;

    // Keep existing behavior: backfill completed_at in done file (even if no moves).
    let backfilled_done = backfill_terminal_completed_at(&mut done, &now) > 0;

    let warnings = validation::validate_queue_set(
        &active,
        Some(&done),
        id_prefix,
        id_width,
        max_dependency_depth,
    )?;
    validation::log_warnings(&warnings);

    if report.moved_ids.is_empty() && !backfilled_done {
        return Ok(report);
    }

    // If only backfill occurred, only save done.
    if report.moved_ids.is_empty() {
        save_queue(done_path, &done)?;
        return Ok(report);
    }

    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;
    Ok(report)
}

/// Archive terminal tasks (Done/Rejected) in-memory and stamp timestamps.
pub fn archive_terminal_tasks_in_memory(
    active: &mut QueueFile,
    done: &mut QueueFile,
    now_rfc3339: &str,
) -> Result<ArchiveReport> {
    let now = super::validate::parse_rfc3339_utc(now_rfc3339)?;
    let mut moved_ids = Vec::new();
    let mut remaining = Vec::with_capacity(active.tasks.len());

    for mut task in active.tasks.drain(..) {
        if matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
            if task
                .completed_at
                .as_ref()
                .is_none_or(|t| t.trim().is_empty())
            {
                task.completed_at = Some(now.clone());
            }
            task.updated_at = Some(now.clone());
            moved_ids.push(task.id.trim().to_string());
            done.tasks.push(task);
        } else {
            remaining.push(task);
        }
    }

    active.tasks = remaining;

    Ok(ArchiveReport { moved_ids })
}

/// Archive terminal tasks (Done/Rejected) from queue to done file.
pub fn archive_terminal_tasks(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<ArchiveReport> {
    let mut active = load_queue(queue_path)?;
    let mut done = load_queue_or_default(done_path)?;

    let now = timeutil::now_utc_rfc3339()?;
    let report = archive_terminal_tasks_in_memory(&mut active, &mut done, &now)?;
    let backfilled_done = backfill_terminal_completed_at(&mut done, &now) > 0;

    // Validate after stamping/moving so missing completed_at on terminal tasks can be repaired.
    let warnings = validation::validate_queue_set(
        &active,
        Some(&done),
        id_prefix,
        id_width,
        max_dependency_depth,
    )?;
    validation::log_warnings(&warnings);

    if report.moved_ids.is_empty() && !backfilled_done {
        return Ok(report);
    }

    if report.moved_ids.is_empty() {
        save_queue(done_path, &done)?;
        return Ok(report);
    }

    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(report)
}
