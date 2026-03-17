//! Purpose: Core done-queue pruning logic and timestamp ordering helpers.
//!
//! Responsibilities:
//! - Load and save the done queue for prune operations.
//! - Apply keep-last, status, and age pruning rules.
//! - Preserve order of retained tasks after pruning.
//! - Provide timestamp parsing and completion-order helpers.
//!
//! Scope:
//! - Prune execution only; CLI parsing and queue facade re-exports live elsewhere.
//!
//! Usage:
//! - Called by `crate::queue::prune_done_tasks` via the prune facade.
//! - Test helpers inject time through `prune_done_queue_at` and `prune_done_tasks_at`.
//!
//! Invariants/Assumptions:
//! - Keep-last protection is index-based to avoid duplicate-ID inflation.
//! - Missing or invalid `completed_at` values are kept for safety in age-based pruning.
//! - Remaining tasks preserve their original relative order after pruning.

use super::super::{load_queue_or_default, save_queue};
use super::types::{PruneOptions, PruneReport};
use crate::contracts::Task;
use crate::timeutil;
use anyhow::Result;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use time::{Duration, OffsetDateTime};

/// Prune tasks from the done archive based on age, status, and keep-last rules.
///
/// This function loads the done archive, applies pruning rules, and optionally
/// saves the result. Pruning preserves the original order of remaining tasks.
///
/// # Arguments
/// * `done_path` - Path to the done archive file
/// * `options` - Pruning options (age filter, status filter, keep-last, dry-run)
///
/// # Returns
/// A `PruneReport` containing the IDs of pruned and kept tasks.
pub fn prune_done_tasks(done_path: &Path, options: PruneOptions) -> Result<PruneReport> {
    let mut done = load_queue_or_default(done_path)?;
    let report = prune_done_queue(&mut done.tasks, &options)?;

    if !options.dry_run && !report.pruned_ids.is_empty() {
        save_queue(done_path, &done)?;
    }

    Ok(report)
}

/// Core pruning logic for a task list.
///
/// Tasks are sorted by completion date (most recent first), then keep-last
/// protection is applied, then age and status filters. The original order of
/// remaining tasks is preserved.
pub(crate) fn prune_done_queue(
    tasks: &mut Vec<Task>,
    options: &PruneOptions,
) -> Result<PruneReport> {
    let now_dt = OffsetDateTime::now_utc();
    prune_done_queue_at(tasks, options, now_dt)
}

pub(crate) fn prune_done_queue_at(
    tasks: &mut Vec<Task>,
    options: &PruneOptions,
    now_dt: OffsetDateTime,
) -> Result<PruneReport> {
    let age_duration = options.age_days.map(|days| Duration::days(days as i64));

    let mut indices: Vec<usize> = (0..tasks.len()).collect();
    indices.sort_by(|&idx_a, &idx_b| compare_completed_desc(&tasks[idx_a], &idx_b, tasks));

    let mut keep_set: HashSet<usize> = HashSet::new();
    if let Some(keep_n) = options.keep_last {
        for &idx in indices.iter().take(keep_n as usize) {
            keep_set.insert(idx);
        }
    }

    let mut pruned_ids = Vec::new();
    let mut kept_ids = Vec::new();
    let mut keep_mask = vec![false; tasks.len()];

    for (idx, task) in tasks.iter().enumerate() {
        if keep_set.contains(&idx) {
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        if !options.statuses.is_empty() && !options.statuses.contains(&task.status) {
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        if let Some(ref completed_at) = task.completed_at {
            if let Some(task_dt) = parse_completed_at(completed_at) {
                if let Some(age_dur) = age_duration {
                    let age = if now_dt >= task_dt {
                        now_dt - task_dt
                    } else {
                        Duration::ZERO
                    };
                    if age < age_dur {
                        keep_mask[idx] = true;
                        kept_ids.push(task.id.clone());
                        continue;
                    }
                }
            } else {
                keep_mask[idx] = true;
                kept_ids.push(task.id.clone());
                continue;
            }
        } else {
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        pruned_ids.push(task.id.clone());
    }

    let mut new_tasks = Vec::new();
    for (idx, task) in tasks.drain(..).enumerate() {
        if keep_mask[idx] {
            new_tasks.push(task);
        }
    }
    *tasks = new_tasks;

    Ok(PruneReport {
        pruned_ids,
        kept_ids,
    })
}

#[cfg(test)]
pub(crate) fn prune_done_tasks_at(
    done_path: &Path,
    options: PruneOptions,
    now_dt: OffsetDateTime,
) -> Result<PruneReport> {
    let mut done = load_queue_or_default(done_path)?;
    let report = prune_done_queue_at(&mut done.tasks, &options, now_dt)?;

    if !options.dry_run && !report.pruned_ids.is_empty() {
        save_queue(done_path, &done)?;
    }

    Ok(report)
}

/// Parse an RFC3339 timestamp into `OffsetDateTime`.
/// Returns `None` if the timestamp is invalid.
fn parse_completed_at(ts: &str) -> Option<OffsetDateTime> {
    timeutil::parse_rfc3339_opt(ts)
}

/// Compare two tasks by completion date for descending sort.
/// Tasks with valid completed_at come first (most recent), then tasks with
/// missing or invalid timestamps (treated as oldest).
fn compare_completed_desc(a: &Task, idx_b: &usize, tasks: &[Task]) -> Ordering {
    let b = &tasks[*idx_b];
    let a_ts = parse_completed_at;
    let b_ts = parse_completed_at;

    match (a.completed_at.as_deref(), b.completed_at.as_deref()) {
        (Some(ts_a), Some(ts_b)) => match (a_ts(ts_a), b_ts(ts_b)) {
            (Some(dt_a), Some(dt_b)) => dt_a.cmp(&dt_b).reverse(),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
