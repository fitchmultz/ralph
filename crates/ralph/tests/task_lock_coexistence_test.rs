//! Tests for task lock coexistence with supervising process.

use ralph::fsutil;
use tempfile::TempDir;

#[test]
fn task_lock_can_be_acquired_when_supervisor_holds_lock() {
    let temp = TempDir::new().expect("create temp dir");
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    let lock_dir = fsutil::queue_lock_dir(repo_root);

    // Supervisor acquires lock with label "run one"
    let supervisor_lock =
        fsutil::acquire_dir_lock(&lock_dir, "run one", false).expect("supervisor lock");
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());

    // Task builder acquires lock with label "task" (shared mode)
    let task_lock = fsutil::acquire_dir_lock(&lock_dir, "task", false).expect("task lock");
    assert!(lock_dir.exists());

    // Verify both owner files exist
    assert!(lock_dir.join("owner").exists());
    assert!(lock_dir
        .join(format!("owner_task_{}", std::process::id()))
        .exists());

    // Drop task lock - verify sidecar removed, supervisor lock remains
    drop(task_lock);
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());
    assert!(!lock_dir
        .join(format!("owner_task_{}", std::process::id()))
        .exists());

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
    let lock_dir = fsutil::queue_lock_dir(repo_root);

    // Supervisor acquires lock with label "run one"
    let _supervisor_lock =
        fsutil::acquire_dir_lock(&lock_dir, "run one", false).expect("supervisor lock");
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());

    // Try to acquire lock with label "task edit" (non-task label) - should fail
    let result = fsutil::acquire_dir_lock(&lock_dir, "task edit", false);
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
    let lock_dir = fsutil::queue_lock_dir(repo_root);

    let pid = std::process::id();
    let sidecar_owner_path = lock_dir.join(format!("owner_task_{}", pid));

    {
        let _task_lock = fsutil::acquire_dir_lock(&lock_dir, "task", false).expect("task lock");
        assert!(lock_dir.exists());
        assert!(sidecar_owner_path.exists());
        assert!(!lock_dir.join("owner").exists());
    }

    // After drop, both the sidecar owner file and lock directory should be gone.
    assert!(!sidecar_owner_path.exists());
    assert!(!lock_dir.exists());
}
