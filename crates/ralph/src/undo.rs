//! Undo system for queue mutations.
//!
//! Responsibilities:
//! - Create snapshots before queue-modifying operations.
//! - List available snapshots for undo.
//! - Restore queue state from snapshots.
//! - Prune old snapshots to enforce retention limits.
//!
//! Not handled here:
//! - CLI argument parsing (see `cli::undo`).
//! - Queue lock acquisition (callers must hold lock).
//!
//! Invariants/assumptions:
//! - Snapshots capture BOTH queue.json and done.json atomically.
//! - Snapshots are written atomically via `fsutil::write_atomic`.
//! - Callers hold queue locks during snapshot creation and restore.

use crate::config::Resolved;
use crate::constants::limits::MAX_UNDO_SNAPSHOTS;
use crate::contracts::QueueFile;
use crate::fsutil;
use crate::queue::{load_queue_or_default, save_queue};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Snapshot filename prefix.
const UNDO_SNAPSHOT_PREFIX: &str = "undo-";

/// Metadata about a single undo snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoSnapshotMeta {
    /// Unique snapshot ID (timestamp-based).
    pub id: String,
    /// Human-readable operation description.
    pub operation: String,
    /// RFC3339 timestamp when snapshot was created.
    pub timestamp: String,
}

/// Full snapshot content (stored in JSON file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoSnapshot {
    /// Schema version for future migrations.
    pub version: u32,
    /// Human-readable operation description.
    pub operation: String,
    /// RFC3339 timestamp when snapshot was created.
    pub timestamp: String,
    /// Full queue.json content at snapshot time.
    pub queue_json: QueueFile,
    /// Full done.json content at snapshot time.
    pub done_json: QueueFile,
}

/// Result of listing snapshots.
#[derive(Debug, Clone)]
pub struct SnapshotList {
    pub snapshots: Vec<UndoSnapshotMeta>,
}

/// Result of a restore operation.
#[derive(Debug, Clone)]
pub struct RestoreResult {
    pub operation: String,
    pub timestamp: String,
    pub tasks_affected: usize,
}

/// Get the undo cache directory path.
pub fn undo_cache_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph").join("cache").join("undo")
}

/// Create a snapshot before a mutation operation.
///
/// This should be called AFTER acquiring the queue lock but BEFORE
/// performing any modifications. The snapshot captures both queue.json
/// and done.json atomically.
///
/// # Arguments
/// * `resolved` - Resolved configuration containing paths
/// * `operation` - Human-readable description of the operation (e.g., "complete_task RQ-0001")
///
/// # Returns
/// Path to the created snapshot file.
pub fn create_undo_snapshot(resolved: &Resolved, operation: &str) -> Result<PathBuf> {
    let undo_dir = undo_cache_dir(&resolved.repo_root);
    std::fs::create_dir_all(&undo_dir)
        .with_context(|| format!("create undo directory {}", undo_dir.display()))?;

    let timestamp = crate::timeutil::now_utc_rfc3339()
        .context("failed to generate timestamp for undo snapshot")?;
    let snapshot_id = timestamp.replace([':', '.', '-'], "");
    let snapshot_filename = format!("{}{}.json", UNDO_SNAPSHOT_PREFIX, snapshot_id);
    let snapshot_path = undo_dir.join(snapshot_filename);

    // Load current state - these should succeed since caller has lock
    let queue_json = load_queue_or_default(&resolved.queue_path)?;
    let done_json = load_queue_or_default(&resolved.done_path)?;

    let snapshot = UndoSnapshot {
        version: 1,
        operation: operation.to_string(),
        timestamp: timestamp.clone(),
        queue_json,
        done_json,
    };

    let content = serde_json::to_string_pretty(&snapshot)?;
    fsutil::write_atomic(&snapshot_path, content.as_bytes())
        .with_context(|| format!("write undo snapshot to {}", snapshot_path.display()))?;

    // Prune old snapshots
    match prune_old_undo_snapshots(&undo_dir, MAX_UNDO_SNAPSHOTS) {
        Ok(pruned) if pruned > 0 => {
            log::debug!("pruned {} old undo snapshot(s)", pruned);
        }
        Ok(_) => {}
        Err(err) => {
            log::warn!("failed to prune undo snapshots: {:#}", err);
        }
    }

    log::debug!(
        "created undo snapshot for '{}' at {}",
        operation,
        snapshot_path.display()
    );

    Ok(snapshot_path)
}

/// List available undo snapshots, newest first.
pub fn list_undo_snapshots(repo_root: &Path) -> Result<SnapshotList> {
    let undo_dir = undo_cache_dir(repo_root);

    if !undo_dir.exists() {
        return Ok(SnapshotList {
            snapshots: Vec::new(),
        });
    }

    let mut snapshots = Vec::new();

    for entry in std::fs::read_dir(&undo_dir)
        .with_context(|| format!("read undo directory {}", undo_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if !path.extension().map(|e| e == "json").unwrap_or(false) {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy();
        if !filename.starts_with(UNDO_SNAPSHOT_PREFIX) {
            continue;
        }

        // Read just the metadata without full content
        match extract_snapshot_meta(&path) {
            Ok(meta) => snapshots.push(meta),
            Err(err) => {
                log::warn!("failed to read snapshot {}: {:#}", path.display(), err);
            }
        }
    }

    // Sort by timestamp descending (newest first)
    snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(SnapshotList { snapshots })
}

/// Extract metadata from a snapshot file without loading full content.
fn extract_snapshot_meta(path: &Path) -> Result<UndoSnapshotMeta> {
    let content = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid snapshot filename: {}", path.display()))?
        .strip_prefix(UNDO_SNAPSHOT_PREFIX)
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid snapshot filename prefix: {}", path.display()))?;

    let operation = value
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp = value
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(UndoSnapshotMeta {
        id,
        operation,
        timestamp,
    })
}

/// Load a full snapshot by ID.
pub fn load_undo_snapshot(repo_root: &Path, snapshot_id: &str) -> Result<UndoSnapshot> {
    let undo_dir = undo_cache_dir(repo_root);
    let snapshot_filename = format!("{}{}.json", UNDO_SNAPSHOT_PREFIX, snapshot_id);
    let snapshot_path = undo_dir.join(snapshot_filename);

    if !snapshot_path.exists() {
        bail!("Snapshot not found: {}", snapshot_id);
    }

    let content = std::fs::read_to_string(&snapshot_path)?;
    let snapshot: UndoSnapshot = serde_json::from_str(&content)?;

    Ok(snapshot)
}

/// Restore queue state from a snapshot.
///
/// This overwrites both queue.json and done.json with the snapshot content.
/// Caller must hold the queue lock.
///
/// # Arguments
/// * `resolved` - Resolved configuration containing paths
/// * `snapshot_id` - ID of snapshot to restore (or None for most recent)
/// * `dry_run` - If true, preview restore without modifying files
///
/// # Returns
/// Information about the restored state.
pub fn restore_from_snapshot(
    resolved: &Resolved,
    snapshot_id: Option<&str>,
    dry_run: bool,
) -> Result<RestoreResult> {
    let list = list_undo_snapshots(&resolved.repo_root)?;

    if list.snapshots.is_empty() {
        bail!("No undo snapshots available");
    }

    let target_id = snapshot_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| list.snapshots[0].id.clone());

    let snapshot = load_undo_snapshot(&resolved.repo_root, &target_id)?;

    let tasks_affected = snapshot.queue_json.tasks.len() + snapshot.done_json.tasks.len();

    if dry_run {
        return Ok(RestoreResult {
            operation: snapshot.operation,
            timestamp: snapshot.timestamp,
            tasks_affected,
        });
    }

    // Perform the restore
    save_queue(&resolved.done_path, &snapshot.done_json)?;
    save_queue(&resolved.queue_path, &snapshot.queue_json)?;

    // Remove the used snapshot (prevents redo cycles)
    let undo_dir = undo_cache_dir(&resolved.repo_root);
    let snapshot_path = undo_dir.join(format!("{}{}.json", UNDO_SNAPSHOT_PREFIX, target_id));
    if let Err(err) = std::fs::remove_file(&snapshot_path) {
        log::warn!("failed to remove used snapshot: {:#}", err);
    }

    log::info!(
        "restored queue state from snapshot '{}' (operation: {}, {} tasks affected)",
        target_id,
        snapshot.operation,
        tasks_affected
    );

    Ok(RestoreResult {
        operation: snapshot.operation,
        timestamp: snapshot.timestamp,
        tasks_affected,
    })
}

/// Prune old snapshots to enforce retention limit.
///
/// Returns the number of snapshots removed.
pub fn prune_old_undo_snapshots(undo_dir: &Path, max_count: usize) -> Result<usize> {
    if max_count == 0 || !undo_dir.exists() {
        return Ok(0);
    }

    let mut snapshot_paths: Vec<PathBuf> = Vec::new();

    for entry in std::fs::read_dir(undo_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.extension().map(|e| e == "json").unwrap_or(false) {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy();
        if filename.starts_with(UNDO_SNAPSHOT_PREFIX) {
            snapshot_paths.push(path);
        }
    }

    if snapshot_paths.len() <= max_count {
        return Ok(0);
    }

    // Sort by filename (which contains timestamp) ascending
    // Oldest files have smallest timestamp
    snapshot_paths.sort_by_key(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    });

    let to_remove = snapshot_paths.len() - max_count;
    let mut removed = 0;

    for path in snapshot_paths.into_iter().take(to_remove) {
        match std::fs::remove_file(&path) {
            Ok(_) => {
                removed += 1;
                log::debug!("pruned old undo snapshot: {}", path.display());
            }
            Err(err) => {
                log::warn!(
                    "failed to remove old snapshot {}: {:#}",
                    path.display(),
                    err
                )
            }
        }
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use crate::queue::load_queue;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_resolved(temp_dir: &TempDir) -> Resolved {
        let repo_root = temp_dir.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        // Create initial queue with one task
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

        // Add a task to done.json
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

        // Get the actual snapshot ID from the created file (strip "undo-" prefix)
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

        // Create multiple snapshots with small delay
        create_undo_snapshot(&resolved, "operation 1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        create_undo_snapshot(&resolved, "operation 2").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        create_undo_snapshot(&resolved, "operation 3").unwrap();

        let list = list_undo_snapshots(&resolved.repo_root).unwrap();
        assert_eq!(list.snapshots.len(), 3);

        // Should be newest first
        assert_eq!(list.snapshots[0].operation, "operation 3");
        assert_eq!(list.snapshots[1].operation, "operation 2");
        assert_eq!(list.snapshots[2].operation, "operation 1");
    }

    #[test]
    fn restore_from_snapshot_restores_both_files() {
        let temp = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp);

        // Create initial snapshot and capture its ID
        let snapshot_path = create_undo_snapshot(&resolved, "initial state").unwrap();
        let snapshot_id = snapshot_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .strip_prefix(UNDO_SNAPSHOT_PREFIX)
            .unwrap()
            .to_string();

        // Modify the queue - add a new task and change existing
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

        // Restore from snapshot using the specific ID
        let result = restore_from_snapshot(&resolved, Some(&snapshot_id), false).unwrap();

        assert_eq!(result.operation, "initial state");
        assert_eq!(result.tasks_affected, 1);

        // Verify queue is restored
        let restored_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(restored_queue.tasks.len(), 1);
        assert_eq!(restored_queue.tasks[0].id, "RQ-0001");
        assert_eq!(restored_queue.tasks[0].status, TaskStatus::Todo);
    }

    #[test]
    fn dry_run_does_not_modify_files() {
        let temp = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp);

        // Create initial snapshot and capture its ID
        let snapshot_path = create_undo_snapshot(&resolved, "initial state").unwrap();
        let snapshot_id = snapshot_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .strip_prefix(UNDO_SNAPSHOT_PREFIX)
            .unwrap()
            .to_string();

        // Modify the queue
        let mut queue = load_queue(&resolved.queue_path).unwrap();
        queue.tasks[0].status = TaskStatus::Doing;
        save_queue(&resolved.queue_path, &queue).unwrap();

        // Restore with dry_run using specific ID
        let result = restore_from_snapshot(&resolved, Some(&snapshot_id), true).unwrap();

        assert_eq!(result.operation, "initial state");

        // Verify queue is NOT restored
        let current_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(current_queue.tasks[0].status, TaskStatus::Doing);
    }

    #[test]
    fn prune_removes_oldest_snapshots() {
        let temp = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp);

        // Create more snapshots than the limit
        for i in 0..(MAX_UNDO_SNAPSHOTS + 5) {
            create_undo_snapshot(&resolved, &format!("operation {}", i)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let list = list_undo_snapshots(&resolved.repo_root).unwrap();
        // Should be limited to MAX_UNDO_SNAPSHOTS
        assert_eq!(list.snapshots.len(), MAX_UNDO_SNAPSHOTS);

        // Most recent should be preserved
        let most_recent = format!("operation {}", MAX_UNDO_SNAPSHOTS + 4);
        assert!(list.snapshots.iter().any(|s| s.operation == most_recent));
    }

    #[test]
    fn restore_with_specific_id() {
        let temp = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp);

        // Create first snapshot
        create_undo_snapshot(&resolved, "first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Create second snapshot and capture its ID
        let second_path = create_undo_snapshot(&resolved, "second").unwrap();
        let second_id = second_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .strip_prefix(UNDO_SNAPSHOT_PREFIX)
            .unwrap()
            .to_string();

        // Modify queue
        let mut queue = load_queue(&resolved.queue_path).unwrap();
        queue.tasks[0].title = "Modified".to_string();
        save_queue(&resolved.queue_path, &queue).unwrap();

        // Restore specific snapshot
        let result = restore_from_snapshot(&resolved, Some(&second_id), false).unwrap();
        assert_eq!(result.operation, "second");
    }

    #[test]
    fn restore_removes_used_snapshot() {
        let temp = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp);

        // Create snapshot and capture its path and ID
        let path = create_undo_snapshot(&resolved, "test").unwrap();
        let id = path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .strip_prefix(UNDO_SNAPSHOT_PREFIX)
            .unwrap()
            .to_string();

        // Restore using specific ID
        restore_from_snapshot(&resolved, Some(&id), false).unwrap();

        // Snapshot should be removed
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
}
