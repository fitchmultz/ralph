//! Purpose: Regression coverage for done-queue pruning behavior.
//!
//! Responsibilities:
//! - Verify age, status, keep-last, dry-run, and order-preservation behavior.
//! - Protect safety-first handling for missing or invalid completion timestamps.
//! - Ensure the facade split preserves the existing prune semantics.
//!
//! Scope:
//! - Unit tests for `queue::prune`; not CLI parsing or unrelated queue workflows.
//!
//! Usage:
//! - Compiled only under `#[cfg(test)]` through `queue/prune/mod.rs`.
//!
//! Invariants/Assumptions:
//! - These tests encode the existing prune contract and should only change with intentional behavior updates.

use super::super::{load_queue, save_queue};
use super::PruneOptions;
use super::core::{prune_done_queue_at, prune_done_tasks_at};
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::timeutil;
use std::collections::{HashMap, HashSet};
use tempfile::TempDir;
use time::OffsetDateTime;

fn fixed_now() -> OffsetDateTime {
    timeutil::parse_rfc3339("2026-01-20T12:00:00Z").expect("fixed timestamp should parse")
}

/// Create a task with a specific completion timestamp.
fn done_task_with_completed(id: &str, completed_at: &str) -> Task {
    let mut task = task_with(id, TaskStatus::Done, vec!["done".to_string()]);
    task.completed_at = Some(completed_at.to_string());
    task
}

/// Create a task without a completion timestamp.
fn done_task_missing_completed(id: &str) -> Task {
    let mut task = task_with(id, TaskStatus::Done, vec!["done".to_string()]);
    task.completed_at = None;
    task
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
        estimated_minutes: None,
        actual_minutes: None,
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
