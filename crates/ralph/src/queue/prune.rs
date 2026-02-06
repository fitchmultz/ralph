//! Queue pruning submodule.
//!
//! This module contains the pruning entry points and internal helpers used to
//! remove old tasks from the done archive (.ralph/done.json) according to
//! configurable filters (age, status, keep-last) while preserving the original
//! order of remaining tasks.
//!
//! The parent `queue` module re-exports the public API so callers can continue
//! using `crate::queue::prune_done_tasks`, `crate::queue::PruneOptions`, and
//! `crate::queue::PruneReport`.

use crate::contracts::{Task, TaskStatus};
use crate::timeutil;
use anyhow::Result;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use time::{Duration, OffsetDateTime};

/// Result of a prune operation on the done archive.
#[derive(Debug, Clone, Default)]
pub struct PruneReport {
    /// IDs of tasks that were pruned (or would be pruned in dry-run).
    pub pruned_ids: Vec<String>,
    /// IDs of tasks that were kept (protected by keep-last or didn't match filters).
    pub kept_ids: Vec<String>,
}

/// Options for pruning tasks from the done archive.
#[derive(Debug, Clone)]
pub struct PruneOptions {
    /// Minimum age in days for a task to be pruned (None = no age filter).
    pub age_days: Option<u32>,
    /// Statuses to prune (empty = all statuses).
    pub statuses: HashSet<TaskStatus>,
    /// Keep the N most recently completed tasks regardless of other filters.
    pub keep_last: Option<u32>,
    /// If true, report what would be pruned without writing to disk.
    pub dry_run: bool,
}

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
    let mut done = super::load_queue_or_default(done_path)?;
    let report = prune_done_queue(&mut done.tasks, &options)?;

    if !options.dry_run && !report.pruned_ids.is_empty() {
        super::save_queue(done_path, &done)?;
    }

    Ok(report)
}

/// Core pruning logic for a task list.
///
/// Tasks are sorted by completion date (most recent first), then keep-last
/// protection is applied, then age and status filters. The original order of
/// remaining tasks is preserved.
fn prune_done_queue(tasks: &mut Vec<Task>, options: &PruneOptions) -> Result<PruneReport> {
    let now = timeutil::now_utc_rfc3339_or_fallback();
    let now_dt = parse_completed_at(&now).unwrap_or_else(OffsetDateTime::now_utc);
    prune_done_queue_at(tasks, options, now_dt)
}

fn prune_done_queue_at(
    tasks: &mut Vec<Task>,
    options: &PruneOptions,
    now_dt: OffsetDateTime,
) -> Result<PruneReport> {
    let age_duration = options.age_days.map(|d| Duration::days(d as i64));

    // Sort indices by completion date descending (most recent first)
    let mut indices: Vec<usize> = (0..tasks.len()).collect();
    indices.sort_by(|&i, &j| compare_completed_desc(&tasks[i], &j, tasks));

    // Apply keep-last protection by index to avoid duplicate-ID inflation
    let mut keep_set: HashSet<usize> = HashSet::new();
    if let Some(keep_n) = options.keep_last {
        for &idx in indices.iter().take(keep_n as usize) {
            keep_set.insert(idx);
        }
    }

    let mut pruned_ids = Vec::new();
    let mut kept_ids = Vec::new();

    // Filter tasks - iterate by index to avoid borrow issues
    let mut keep_mask = vec![false; tasks.len()];
    for (idx, task) in tasks.iter().enumerate() {
        // Check keep-last protection first
        if keep_set.contains(&idx) {
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        // Check status filter
        if !options.statuses.is_empty() && !options.statuses.contains(&task.status) {
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        // Check age filter
        if let Some(ref completed_at) = task.completed_at {
            if let Some(task_dt) = parse_completed_at(completed_at) {
                if let Some(age_dur) = age_duration {
                    // Calculate age: now - task_dt
                    // Use checked_sub to handle potential underflow gracefully
                    let age = if now_dt >= task_dt {
                        now_dt - task_dt
                    } else {
                        // Task is in the future (clock skew), treat as 0 age
                        Duration::ZERO
                    };
                    if age < age_dur {
                        // Too recent to prune
                        keep_mask[idx] = true;
                        kept_ids.push(task.id.clone());
                        continue;
                    }
                }
            } else {
                // Invalid completed_at - keep for safety (don't prune by age)
                keep_mask[idx] = true;
                kept_ids.push(task.id.clone());
                continue;
            }
        } else {
            // Missing completed_at - keep for safety (don't prune by age)
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        // Task passes all filters - mark for pruning
        pruned_ids.push(task.id.clone());
    }

    // Remove pruned tasks while preserving order
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
fn prune_done_tasks_at(
    done_path: &Path,
    options: PruneOptions,
    now_dt: OffsetDateTime,
) -> Result<PruneReport> {
    let mut done = super::load_queue_or_default(done_path)?;
    let report = prune_done_queue_at(&mut done.tasks, &options, now_dt)?;

    if !options.dry_run && !report.pruned_ids.is_empty() {
        super::save_queue(done_path, &done)?;
    }

    Ok(report)
}

/// Parse an RFC3339 timestamp into OffsetDateTime.
/// Returns None if the timestamp is invalid.
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

#[cfg(test)]
mod tests {
    //! Pruning behavior tests (age/status/keep-last, safety, and order preservation).

    use super::super::{load_queue, save_queue};
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use std::collections::{HashMap, HashSet};
    use tempfile::TempDir;

    fn fixed_now() -> OffsetDateTime {
        timeutil::parse_rfc3339("2026-01-20T12:00:00Z").expect("fixed timestamp should parse")
    }

    /// Create a task with a specific completion timestamp.
    fn done_task_with_completed(id: &str, completed_at: &str) -> Task {
        let mut t = task_with(id, TaskStatus::Done, vec!["done".to_string()]);
        t.completed_at = Some(completed_at.to_string());
        t
    }

    /// Create a task without a completion timestamp.
    fn done_task_missing_completed(id: &str) -> Task {
        let mut t = task_with(id, TaskStatus::Done, vec!["done".to_string()]);
        t.completed_at = None;
        t
    }

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags,
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn prune_by_age_only() {
        // now = 2026-01-20T12:00:00Z
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"),
            done_task_with_completed("RQ-0003", "2026-01-19T12:00:00Z"),
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(15),
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue_at(&mut done.tasks, &options, fixed_now()).unwrap();

        assert_eq!(report.pruned_ids, vec!["RQ-0001"]);
        assert_eq!(report.kept_ids.len(), 2);
        assert!(report.kept_ids.contains(&"RQ-0002".to_string()));
        assert!(report.kept_ids.contains(&"RQ-0003".to_string()));
        assert_eq!(done.tasks.len(), 2);
    }

    #[test]
    fn prune_by_status_only() {
        let mut tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"),
            task_with("RQ-0003", TaskStatus::Rejected, vec!["done".to_string()]),
        ];
        tasks[2].completed_at = Some("2026-01-15T12:00:00Z".to_string());

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: None,
            statuses: vec![TaskStatus::Rejected].into_iter().collect(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue_at(&mut done.tasks, &options, fixed_now()).unwrap();

        assert_eq!(report.pruned_ids, vec!["RQ-0003"]);
        assert_eq!(report.kept_ids.len(), 2);
        assert_eq!(done.tasks.len(), 2);
    }

    #[test]
    fn prune_keep_last_protects_recent() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"),
            done_task_with_completed("RQ-0003", "2026-01-15T12:00:00Z"),
            done_task_with_completed("RQ-0004", "2026-01-19T12:00:00Z"),
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: None,
            statuses: HashSet::new(),
            keep_last: Some(2),
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue_at(&mut done.tasks, &options, fixed_now()).unwrap();

        assert_eq!(report.kept_ids.len(), 2);
        assert!(report.kept_ids.contains(&"RQ-0003".to_string()));
        assert!(report.kept_ids.contains(&"RQ-0004".to_string()));
        assert_eq!(report.pruned_ids.len(), 2);
        assert!(report.pruned_ids.contains(&"RQ-0001".to_string()));
        assert!(report.pruned_ids.contains(&"RQ-0002".to_string()));
        assert_eq!(done.tasks.len(), 2);
    }

    #[test]
    fn prune_keep_last_with_duplicate_ids() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"),
            done_task_with_completed("RQ-0003", "2026-01-15T12:00:00Z"),
            done_task_with_completed("RQ-0003", "2026-01-19T12:00:00Z"),
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: None,
            statuses: HashSet::new(),
            keep_last: Some(2),
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue_at(&mut done.tasks, &options, fixed_now()).unwrap();

        assert_eq!(report.kept_ids.len(), 2);
        assert_eq!(report.pruned_ids.len(), 2);
        assert_eq!(done.tasks.len(), 2);
        assert_eq!(done.tasks[0].id, "RQ-0003");
        assert_eq!(done.tasks[1].id, "RQ-0003");
        assert_eq!(report.kept_ids, vec!["RQ-0003", "RQ-0003"]);
        assert_eq!(report.pruned_ids, vec!["RQ-0001", "RQ-0002"]);
    }

    #[test]
    fn prune_combined_age_and_status() {
        let mut tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"),
            task_with("RQ-0003", TaskStatus::Rejected, vec!["done".to_string()]),
            task_with("RQ-0004", TaskStatus::Rejected, vec!["done".to_string()]),
        ];
        tasks[2].completed_at = Some("2026-01-05T12:00:00Z".to_string());
        tasks[3].completed_at = Some("2026-01-15T12:00:00Z".to_string());

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(10),
            statuses: vec![TaskStatus::Rejected].into_iter().collect(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue_at(&mut done.tasks, &options, fixed_now()).unwrap();

        assert_eq!(report.pruned_ids, vec!["RQ-0003"]);
        assert_eq!(report.kept_ids.len(), 3);
        assert_eq!(done.tasks.len(), 3);
    }

    #[test]
    fn prune_missing_completed_at_kept_for_safety() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_missing_completed("RQ-0002"),
            done_task_with_completed("RQ-0003", "2026-01-18T12:00:00Z"),
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5),
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue_at(&mut done.tasks, &options, fixed_now()).unwrap();

        assert_eq!(report.pruned_ids, vec!["RQ-0001"]);
        assert_eq!(report.kept_ids.len(), 2);
        assert!(report.kept_ids.contains(&"RQ-0002".to_string()));
        assert!(report.kept_ids.contains(&"RQ-0003".to_string()));
        assert_eq!(done.tasks.len(), 2);
    }

    #[test]
    fn prune_dry_run_does_not_write_to_disk() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-18T12:00:00Z"),
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5),
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: true,
        };

        let report = prune_done_tasks_at(&done_path, options, fixed_now()).unwrap();

        assert_eq!(report.pruned_ids, vec!["RQ-0001"]);

        let done_after = load_queue(&done_path).unwrap();
        assert_eq!(done_after.tasks.len(), 2);
    }

    #[test]
    fn prune_preserves_original_order() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-16T12:00:00Z"),
            done_task_with_completed("RQ-0003", "2026-01-18T12:00:00Z"),
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5),
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: false,
        };

        prune_done_tasks_at(&done_path, options, fixed_now()).unwrap();

        let done_after = load_queue(&done_path).unwrap();
        assert_eq!(done_after.tasks.len(), 2);
        assert_eq!(done_after.tasks[0].id, "RQ-0002");
        assert_eq!(done_after.tasks[1].id, "RQ-0003");
    }

    #[test]
    fn prune_with_keep_last_and_age_combines_filters() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"),
            done_task_with_completed("RQ-0003", "2026-01-15T12:00:00Z"),
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5),
            statuses: HashSet::new(),
            keep_last: Some(1),
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue_at(&mut done.tasks, &options, fixed_now()).unwrap();

        assert_eq!(report.pruned_ids.len(), 2);
        assert!(report.pruned_ids.contains(&"RQ-0001".to_string()));
        assert!(report.pruned_ids.contains(&"RQ-0002".to_string()));
        assert_eq!(report.kept_ids, vec!["RQ-0003"]);
        assert_eq!(done.tasks.len(), 1);
    }

    #[test]
    fn prune_invalid_completed_at_kept_for_safety() {
        let mut tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            task_with("RQ-0002", TaskStatus::Done, vec!["done".to_string()]),
        ];
        tasks[1].completed_at = Some("not-a-valid-timestamp".to_string());

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5),
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue_at(&mut done.tasks, &options, fixed_now()).unwrap();

        assert_eq!(report.pruned_ids, vec!["RQ-0001"]);
        assert_eq!(report.kept_ids, vec!["RQ-0002"]);
        assert_eq!(done.tasks.len(), 1);
    }
}
