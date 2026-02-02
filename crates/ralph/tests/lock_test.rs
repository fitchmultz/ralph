//! Tests for directory lock helpers.
//!
//! Responsibilities:
//! - Validate lock acquisition, ownership metadata, and cleanup semantics.
//! - Validate shared lock behavior and error reporting.
//!
//! Not covered here:
//! - Temp directory helpers or atomic writes (see `fsutil_test.rs`).
//! - CLI-level queue workflows.
//!
//! Invariants/assumptions:
//! - Tests run on local filesystem with temp directories.
//! - PID liveness detection may be platform-dependent.

use ralph::lock;
use std::fs;
use std::thread;
use tempfile::TempDir;

#[cfg(unix)]
mod lock_support;

#[test]
fn test_queue_lock_dir() {
    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();

    let lock_dir = lock::queue_lock_dir(repo_root);
    assert_eq!(lock_dir, repo_root.join(".ralph").join("lock"));
}

#[test]
fn test_acquire_dir_lock_success() {
    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let lock_dir = lock::queue_lock_dir(repo_root);

    let lock = lock::acquire_dir_lock(&lock_dir, "test_label", false).unwrap();

    // Verify lock directory exists
    assert!(lock_dir.exists());
    assert!(lock_dir.is_dir());

    // Verify owner file exists
    let owner_path = lock_dir.join("owner");
    assert!(owner_path.exists());

    // Verify owner file contains expected content
    let owner_content = fs::read_to_string(&owner_path).unwrap();
    assert!(owner_content.contains("pid:"));
    assert!(owner_content.contains("label: test_label"));
    assert!(owner_content.contains("started_at:"));
    assert!(owner_content.contains("command:"));

    // Lock is released when dropped
    drop(lock);
    assert!(!lock_dir.exists());
}

#[test]
fn test_acquire_dir_lock_already_held() {
    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let lock_dir = lock::queue_lock_dir(repo_root);

    let _lock1 = lock::acquire_dir_lock(&lock_dir, "first", false).unwrap();

    // Second acquisition should fail
    let result = lock::acquire_dir_lock(&lock_dir, "second", false);
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Queue lock already held"));
    assert!(err_msg.contains("first"));
}

#[cfg(unix)]
#[test]
fn test_acquire_dir_lock_force_with_stale_pid() {
    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let lock_dir = lock::queue_lock_dir(repo_root);

    // Create a stale lock manually
    fs::create_dir_all(&lock_dir).unwrap();
    let owner_path = lock_dir.join("owner");
    let stale_pid = lock_support::spawn_exited_pid();
    let fake_owner = format!(
        "pid: {stale_pid}\nstarted_at: 2025-01-19T00:00:00Z\ncommand: test\nlabel: stale\n"
    );
    fs::write(&owner_path, fake_owner).unwrap();

    // Force acquisition should clear stale lock
    let lock = lock::acquire_dir_lock(&lock_dir, "new_label", true).unwrap();
    assert!(lock_dir.exists());

    // Verify owner was updated
    let owner_content = fs::read_to_string(&owner_path).unwrap();
    assert!(owner_content.contains("label: new_label"));

    drop(lock);
    assert!(!lock_dir.exists());
}

#[test]
fn test_acquire_dir_lock_creates_parent_dir() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("nested").join(".ralph").join("lock");

    let lock = lock::acquire_dir_lock(&lock_dir, "test", false).unwrap();
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());

    drop(lock);
    // DirLock only removes the lock directory itself, not parent directories
    assert!(!lock_dir.exists());
    assert!(lock_dir.parent().unwrap().exists());
}

#[test]
fn test_acquire_dir_lock_empty_label_uses_default() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    let lock = lock::acquire_dir_lock(&lock_dir, "", false).unwrap();

    let owner_path = lock_dir.join("owner");
    let owner_content = fs::read_to_string(&owner_path).unwrap();
    assert!(owner_content.contains("label: unspecified"));

    drop(lock);
}

#[test]
fn test_acquire_dir_lock_whitespace_label_gets_trimmed() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    let lock = lock::acquire_dir_lock(&lock_dir, "  test_label  ", false).unwrap();

    let owner_path = lock_dir.join("owner");
    let owner_content = fs::read_to_string(&owner_path).unwrap();
    assert!(owner_content.contains("label: test_label"));

    drop(lock);
}

#[test]
fn test_dir_lock_drop_cleans_up() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    {
        let _lock = lock::acquire_dir_lock(&lock_dir, "test", false).unwrap();
        assert!(lock_dir.exists());
    }

    // After dropping, lock directory should be removed
    assert!(!lock_dir.exists());
}

#[test]
fn test_acquire_dir_lock_concurrent() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    let lock1 = lock::acquire_dir_lock(&lock_dir, "lock1", false).unwrap();

    // Try to acquire the same lock from another thread
    let lock_dir_clone = lock_dir.clone();
    let handle = thread::spawn(move || lock::acquire_dir_lock(&lock_dir_clone, "lock2", false));

    let result = handle.join().unwrap();
    assert!(result.is_err());

    drop(lock1);

    // Now should be able to acquire
    let lock2 = lock::acquire_dir_lock(&lock_dir, "lock2", false).unwrap();
    assert!(lock_dir.exists());

    drop(lock2);
}

#[test]
fn test_parse_lock_owner_valid() {
    let _raw =
        "pid: 12345\nstarted_at: 2025-01-19T00:00:00Z\ncommand: ralph test\nlabel: test_label";
    // This is tested indirectly through acquire_dir_lock
    // Direct testing would require making parse_lock_owner public
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    let lock = lock::acquire_dir_lock(&lock_dir, "test_label", false).unwrap();

    let owner_path = lock_dir.join("owner");
    let content = fs::read_to_string(&owner_path).unwrap();
    assert!(content.contains("pid:"));
    assert!(content.contains("started_at:"));
    assert!(content.contains("command:"));
    assert!(content.contains("label: test_label"));

    drop(lock);
}

#[test]
fn test_parse_lock_owner_with_extra_whitespace() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    let lock = lock::acquire_dir_lock(&lock_dir, "  spaced_label  ", false).unwrap();

    let owner_path = lock_dir.join("owner");
    let content = fs::read_to_string(&owner_path).unwrap();
    assert!(content.contains("label: spaced_label"));
    assert!(!content.contains("spaced_label  "));

    drop(lock);
}

#[test]
fn test_lock_owner_renders_current_process_info() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    let lock = lock::acquire_dir_lock(&lock_dir, "process_info", false).unwrap();

    let owner_path = lock_dir.join("owner");
    let content = fs::read_to_string(&owner_path).unwrap();

    // Should contain current process ID
    let current_pid = std::process::id();
    assert!(content.contains(&format!("pid: {}", current_pid)));

    // Should have started_at timestamp
    assert!(content.contains("started_at:"));
    assert!(content.contains("20")); // Year starts with 20

    // Should have command line
    assert!(content.contains("command:"));

    drop(lock);
}
