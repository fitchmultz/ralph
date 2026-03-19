//! Purpose: Verify undo snapshot persistence, ordering, restore, and pruning.
//!
//! Responsibilities:
//! - Cover snapshot creation/listing/loading behavior.
//! - Cover restore semantics for dry-run and applied restores.
//! - Cover retention pruning and cache-directory helpers.
//!
//! Scope:
//! - Regression tests only; undo implementation lives in sibling modules.
//!
//! Usage:
//! - Run via `cargo test -p ralph-agent-loop undo` or the broader CI gates.
//!
//! Invariants/Assumptions:
//! - Test snapshots use isolated temp directories.
//! - Snapshot IDs continue to derive from the `undo-<timestamp>.json` naming convention.

use std::collections::HashMap;

use tempfile::TempDir;

use crate::config::Resolved;
use crate::constants::limits::MAX_UNDO_SNAPSHOTS;
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue::{load_queue, save_queue};

use super::{
    UNDO_SNAPSHOT_PREFIX, create_undo_snapshot, list_undo_snapshots, load_undo_snapshot,
    restore_from_snapshot, undo_cache_dir,
};

fn create_test_resolved(temp_dir: &TempDir) -> Resolved {
    let repo_root = temp_dir.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).unwrap();

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let queue = QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0001".to_string(),
            title: "Test task".to_string(),
            status: TaskStatus::Todo,
            description: None,
            priority: Default::default(),
            tags: vec!["test".to_string()],
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
            estimated_minutes: None,
            actual_minutes: None,
        }],
    };

    save_queue(&queue_path, &queue).unwrap();

    Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.to_path_buf(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

#[test]
fn create_undo_snapshot_creates_file() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    let snapshot_path = create_undo_snapshot(&resolved, "test operation").unwrap();

    assert!(snapshot_path.exists());
    assert!(snapshot_path.to_string_lossy().contains("undo-"));
}

#[test]
fn snapshot_contains_both_queues() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    let done = QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0000".to_string(),
            title: "Done task".to_string(),
            status: TaskStatus::Done,
            description: None,
            priority: Default::default(),
            tags: vec!["done".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["done thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-17T00:00:00Z".to_string()),
            updated_at: Some("2026-01-17T00:00:00Z".to_string()),
            completed_at: Some("2026-01-17T12:00:00Z".to_string()),
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
            estimated_minutes: None,
            actual_minutes: None,
        }],
    };
    save_queue(&resolved.done_path, &done).unwrap();

    let snapshot_path = create_undo_snapshot(&resolved, "test operation").unwrap();
    let list = list_undo_snapshots(&resolved.repo_root).unwrap();
    assert_eq!(list.snapshots.len(), 1);

    let actual_id = snapshot_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .strip_prefix(UNDO_SNAPSHOT_PREFIX)
        .unwrap()
        .to_string();

    let snapshot = load_undo_snapshot(&resolved.repo_root, &actual_id).unwrap();
    assert_eq!(snapshot.queue_json.tasks.len(), 1);
    assert_eq!(snapshot.queue_json.tasks[0].id, "RQ-0001");
    assert_eq!(snapshot.done_json.tasks.len(), 1);
    assert_eq!(snapshot.done_json.tasks[0].id, "RQ-0000");
}

#[test]
fn list_snapshots_returns_newest_first() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    create_undo_snapshot(&resolved, "operation 1").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    create_undo_snapshot(&resolved, "operation 2").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    create_undo_snapshot(&resolved, "operation 3").unwrap();

    let list = list_undo_snapshots(&resolved.repo_root).unwrap();
    assert_eq!(list.snapshots.len(), 3);
    assert_eq!(list.snapshots[0].operation, "operation 3");
    assert_eq!(list.snapshots[1].operation, "operation 2");
    assert_eq!(list.snapshots[2].operation, "operation 1");
}

#[test]
fn restore_from_snapshot_restores_both_files() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    let snapshot_path = create_undo_snapshot(&resolved, "initial state").unwrap();
    let snapshot_id = snapshot_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .strip_prefix(UNDO_SNAPSHOT_PREFIX)
        .unwrap()
        .to_string();

    let mut queue = load_queue(&resolved.queue_path).unwrap();
    queue.tasks[0].status = TaskStatus::Doing;
    queue.tasks.push(Task {
        id: "RQ-0002".to_string(),
        title: "New task".to_string(),
        status: TaskStatus::Todo,
        description: None,
        priority: Default::default(),
        tags: vec!["new".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["new thing".to_string()],
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
        estimated_minutes: None,
        actual_minutes: None,
    });
    save_queue(&resolved.queue_path, &queue).unwrap();

    let result = restore_from_snapshot(&resolved, Some(&snapshot_id), false).unwrap();
    assert_eq!(result.operation, "initial state");
    assert_eq!(result.tasks_affected, 1);

    let restored_queue = load_queue(&resolved.queue_path).unwrap();
    assert_eq!(restored_queue.tasks.len(), 1);
    assert_eq!(restored_queue.tasks[0].id, "RQ-0001");
    assert_eq!(restored_queue.tasks[0].status, TaskStatus::Todo);
}

#[test]
fn dry_run_does_not_modify_files() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    let snapshot_path = create_undo_snapshot(&resolved, "initial state").unwrap();
    let snapshot_id = snapshot_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .strip_prefix(UNDO_SNAPSHOT_PREFIX)
        .unwrap()
        .to_string();

    let mut queue = load_queue(&resolved.queue_path).unwrap();
    queue.tasks[0].status = TaskStatus::Doing;
    save_queue(&resolved.queue_path, &queue).unwrap();

    let result = restore_from_snapshot(&resolved, Some(&snapshot_id), true).unwrap();
    assert_eq!(result.operation, "initial state");

    let current_queue = load_queue(&resolved.queue_path).unwrap();
    assert_eq!(current_queue.tasks[0].status, TaskStatus::Doing);
}

#[test]
fn prune_removes_oldest_snapshots() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    for index in 0..(MAX_UNDO_SNAPSHOTS + 5) {
        create_undo_snapshot(&resolved, &format!("operation {index}")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let list = list_undo_snapshots(&resolved.repo_root).unwrap();
    assert_eq!(list.snapshots.len(), MAX_UNDO_SNAPSHOTS);

    let most_recent = format!("operation {}", MAX_UNDO_SNAPSHOTS + 4);
    assert!(
        list.snapshots
            .iter()
            .any(|snapshot| snapshot.operation == most_recent)
    );
}

#[test]
fn restore_with_specific_id() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    create_undo_snapshot(&resolved, "first").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));

    let second_path = create_undo_snapshot(&resolved, "second").unwrap();
    let second_id = second_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .strip_prefix(UNDO_SNAPSHOT_PREFIX)
        .unwrap()
        .to_string();

    let mut queue = load_queue(&resolved.queue_path).unwrap();
    queue.tasks[0].title = "Modified".to_string();
    save_queue(&resolved.queue_path, &queue).unwrap();

    let result = restore_from_snapshot(&resolved, Some(&second_id), false).unwrap();
    assert_eq!(result.operation, "second");
}

#[test]
fn restore_removes_used_snapshot() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    let path = create_undo_snapshot(&resolved, "test").unwrap();
    let id = path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .strip_prefix(UNDO_SNAPSHOT_PREFIX)
        .unwrap()
        .to_string();

    restore_from_snapshot(&resolved, Some(&id), false).unwrap();

    let list = list_undo_snapshots(&resolved.repo_root).unwrap();
    assert!(list.snapshots.is_empty());
    assert!(!path.exists());
}

#[test]
fn restore_no_snapshots_error() {
    let temp = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp);

    let result = restore_from_snapshot(&resolved, None, false);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("No undo snapshots")
    );
}

#[test]
fn undo_cache_dir_creates_correct_path() {
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path();

    let dir = undo_cache_dir(repo_root);
    assert!(dir.to_string_lossy().contains(".ralph/cache/undo"));
}
