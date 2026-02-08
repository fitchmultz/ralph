//! Tests for task lock coexistence with supervising process.
//!
//! Responsibilities:
//! - Verify shared lock behavior for task labels under supervision.
//! - Ensure supervisor and task owners can coexist safely.
//! - Test multiple concurrent task locks from the same process.
//!
//! Not covered here:
//! - General lock ownership metadata formatting (see `lock_test.rs`).
//! - Temp directory helpers or atomic writes.
//!
//! Invariants/assumptions:
//! - Lock directory is a local filesystem path under a temp repo.
//! - Supervising labels retain exclusive semantics outside shared task mode.
//! - Task sidecar files use unique naming (owner_task_<pid>_<counter>).

use ralph::lock;
use std::fs;
use tempfile::TempDir;

/// Helper to check if any task owner files exist in the lock directory
fn get_task_owner_files(lock_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    if !lock_dir.exists() {
        return vec![];
    }
    fs::read_dir(lock_dir)
        .expect("read lock dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|name| name.starts_with("owner_task_"))
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect()
}

#[test]
fn task_lock_can_be_acquired_when_supervisor_holds_lock() {
    let temp = TempDir::new().expect("create temp dir");
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    let lock_dir = lock::queue_lock_dir(repo_root);

    // Supervisor acquires lock with label "run one"
    let supervisor_lock =
        lock::acquire_dir_lock(&lock_dir, "run one", false).expect("supervisor lock");
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());

    // Task builder acquires lock with label "task" (shared mode)
    let task_lock = lock::acquire_dir_lock(&lock_dir, "task", false).expect("task lock");
    assert!(lock_dir.exists());

    // Verify both owner files exist
    assert!(lock_dir.join("owner").exists());
    let task_owner_files = get_task_owner_files(&lock_dir);
    assert_eq!(
        task_owner_files.len(),
        1,
        "Expected exactly one task owner file"
    );

    // Drop task lock - verify sidecar removed, supervisor lock remains
    drop(task_lock);
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());
    assert!(
        get_task_owner_files(&lock_dir).is_empty(),
        "Task owner files should be cleaned up"
    );

    // Drop supervisor lock - verify directory removed
    drop(supervisor_lock);
    assert!(!lock_dir.exists());
}

#[test]
fn non_task_lock_still_fails_when_supervisor_holds_lock() {
    let temp = TempDir::new().expect("create temp dir");
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    let lock_dir = lock::queue_lock_dir(repo_root);

    // Supervisor acquires lock with label "run one"
    let _supervisor_lock =
        lock::acquire_dir_lock(&lock_dir, "run one", false).expect("supervisor lock");
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());

    // Try to acquire lock with label "task edit" (non-task label) - should fail
    let result = lock::acquire_dir_lock(&lock_dir, "task edit", false);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Queue lock already held"));
    assert!(err_msg.contains("run one"));
}

#[test]
fn task_lock_cleans_up_directory_when_no_supervisor() {
    let temp = TempDir::new().expect("create temp dir");
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    let lock_dir = lock::queue_lock_dir(repo_root);

    {
        let _task_lock = lock::acquire_dir_lock(&lock_dir, "task", false).expect("task lock");
        assert!(lock_dir.exists());
        let task_owner_files = get_task_owner_files(&lock_dir);
        assert_eq!(task_owner_files.len(), 1);
        assert!(!lock_dir.join("owner").exists());
    }

    // After drop, both the sidecar owner file and lock directory should be gone.
    assert!(
        get_task_owner_files(&lock_dir).is_empty(),
        "Task owner files should be cleaned up"
    );
    assert!(!lock_dir.exists());
}

#[test]
fn multiple_task_locks_in_same_process_have_unique_sidecars() {
    let temp = TempDir::new().expect("create temp dir");
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    let lock_dir = lock::queue_lock_dir(repo_root);

    // Supervisor acquires lock first
    let supervisor_lock =
        lock::acquire_dir_lock(&lock_dir, "run loop", false).expect("supervisor lock");
    assert!(lock_dir.join("owner").exists());

    // Acquire multiple task locks from the same process
    let task_lock1 = lock::acquire_dir_lock(&lock_dir, "task", false).expect("task lock 1");
    let task_lock2 = lock::acquire_dir_lock(&lock_dir, "task", false).expect("task lock 2");
    let task_lock3 = lock::acquire_dir_lock(&lock_dir, "task", false).expect("task lock 3");

    // Verify all task locks have unique sidecar files
    let task_owner_files = get_task_owner_files(&lock_dir);
    assert_eq!(
        task_owner_files.len(),
        3,
        "Expected three unique task owner files, found: {:?}",
        task_owner_files
    );

    // Verify the main owner file still exists
    assert!(lock_dir.join("owner").exists());

    // Drop the task locks in reverse order
    drop(task_lock3);

    // Directory and remaining owners should still exist
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());
    assert_eq!(
        get_task_owner_files(&lock_dir).len(),
        2,
        "Expected two task owner files remaining"
    );

    drop(task_lock2);

    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());
    assert_eq!(
        get_task_owner_files(&lock_dir).len(),
        1,
        "Expected one task owner file remaining"
    );

    drop(task_lock1);

    // After all task locks are dropped, the sidecars should be gone but
    // the directory should still exist (supervisor still holds it)
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());
    assert!(
        get_task_owner_files(&lock_dir).is_empty(),
        "All task owner files should be cleaned up"
    );

    // Drop supervisor - directory should be cleaned up
    drop(supervisor_lock);
    assert!(!lock_dir.exists());
}

#[test]
fn concurrent_task_locks_release_independently() {
    let temp = TempDir::new().expect("create temp dir");
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    let lock_dir = lock::queue_lock_dir(repo_root);

    // First, acquire a supervising lock.
    let supervisor_lock =
        lock::acquire_dir_lock(&lock_dir, "run loop", false).expect("supervisor lock");
    assert!(lock_dir.exists());

    // Acquire multiple task locks
    let task_lock1 = lock::acquire_dir_lock(&lock_dir, "task", false).expect("task lock 1");
    let task_lock2 = lock::acquire_dir_lock(&lock_dir, "task", false).expect("task lock 2");

    // Get the owner file paths before dropping
    let owner_files_before: Vec<_> = get_task_owner_files(&lock_dir);
    assert_eq!(owner_files_before.len(), 2);

    // Drop one task lock
    drop(task_lock1);

    // Verify one task owner file is removed but the other remains
    let owner_files_after = get_task_owner_files(&lock_dir);
    assert_eq!(
        owner_files_after.len(),
        1,
        "Expected one task owner file to remain"
    );

    // The remaining file should be different from the removed one
    assert_ne!(
        owner_files_before, owner_files_after,
        "The remaining file should be different from the set before"
    );

    // Directory should still exist with supervisor and one task sidecar
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());

    // Drop remaining locks
    drop(task_lock2);
    drop(supervisor_lock);

    // Everything should be cleaned up
    assert!(!lock_dir.exists());
}

#[test]
fn task_lock_handles_supervisor_drop_before_task() {
    let temp = TempDir::new().expect("create temp dir");
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    let lock_dir = lock::queue_lock_dir(repo_root);

    // Supervisor acquires lock
    let supervisor_lock =
        lock::acquire_dir_lock(&lock_dir, "run one", false).expect("supervisor lock");

    // Task lock acquired
    let task_lock = lock::acquire_dir_lock(&lock_dir, "task", false).expect("task lock");

    // Verify both exist
    assert!(lock_dir.join("owner").exists());
    assert_eq!(get_task_owner_files(&lock_dir).len(), 1);

    // Drop supervisor first (unusual but should be handled)
    drop(supervisor_lock);

    // Directory and task sidecar should still exist
    assert!(lock_dir.exists());
    assert!(
        !lock_dir.join("owner").exists(),
        "Main owner file should be gone"
    );
    assert_eq!(
        get_task_owner_files(&lock_dir).len(),
        1,
        "Task owner should remain"
    );

    // Drop task lock - since it's the last one, directory should be cleaned up
    drop(task_lock);
    assert!(!lock_dir.exists());
}
