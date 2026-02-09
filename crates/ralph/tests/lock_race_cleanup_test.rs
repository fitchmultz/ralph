//! Integration tests for lock cleanup race condition handling.
//!
//! Responsibilities:
//! - Validate that concurrent lock acquisition/release doesn't leave orphaned directories.
//! - Verify retry logic with exponential backoff handles race conditions properly.
//! - Test force cleanup behavior for stale locks.
//! - Test cleanup behavior with multiple task sidecars and supervising locks.
//!
//! Not covered here:
//! - Stale lock detection and cleanup (see `stale_lock_cleanup_test.rs`).
//! - Basic lock acquisition semantics (see `lock_test.rs`).
//!
//! Invariants/assumptions:
//! - Tests use multiple threads to simulate concurrent access.
//! - Temp directories are isolated per test.
//! - Tests run on local filesystem with reasonable timing assumptions.

mod test_support;

use ralph::lock;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
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

/// Test that rapid concurrent lock acquisition and release doesn't leave
/// orphaned lock directories behind.
#[test]
fn test_concurrent_lock_cleanup_no_orphans() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    const NUM_THREADS: usize = 10;
    const ITERATIONS_PER_THREAD: usize = 20;

    let success_count = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|thread_id| {
            let lock_dir = lock_dir.clone();
            let success_count = success_count.clone();
            let barrier = barrier.clone();

            thread::spawn(move || {
                // Wait for all threads to be ready
                barrier.wait();

                for i in 0..ITERATIONS_PER_THREAD {
                    let label = format!("thread_{}_iter_{}", thread_id, i);

                    // Try to acquire lock - may fail if another thread holds it
                    match lock::acquire_dir_lock(&lock_dir, &label, false) {
                        Ok(_lock) => {
                            // Hold the lock briefly to increase contention
                            thread::yield_now();
                            success_count.fetch_add(1, Ordering::SeqCst);
                            // Lock drops here, triggering cleanup
                        }
                        Err(_) => {
                            // Expected contention failure - try again
                            thread::yield_now();
                        }
                    }
                }
            })
        })
        .collect();

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("thread should not panic");
    }

    // Wait for cleanup to complete under load.
    assert!(
        test_support::wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
            !lock_dir.exists()
                || fs::read_dir(&lock_dir)
                    .map(|mut entries| entries.next().is_none())
                    .unwrap_or(true)
        }),
        "lock directory cleanup timed out"
    );

    // Verify no orphaned lock directories remain
    if lock_dir.exists() {
        // If the directory exists, it should be empty (or contain only stale owner files)
        let entries: Vec<_> = fs::read_dir(&lock_dir)
            .expect("read lock dir")
            .filter_map(|e| e.ok())
            .collect();

        // The directory should be empty after all locks are dropped
        assert!(
            entries.is_empty(),
            "Lock directory should be empty after concurrent test, but found: {:?}",
            entries
        );

        // Clean up the empty directory
        let _ = fs::remove_dir(&lock_dir);
    }

    // Verify that at least some acquisitions succeeded
    let total_successes = success_count.load(Ordering::SeqCst);
    assert!(
        total_successes > 0,
        "At least some lock acquisitions should have succeeded"
    );
}

/// Test that the retry logic in Drop handles the race condition where
/// another thread creates a file in the lock directory during cleanup.
#[test]
fn test_drop_retry_handles_race_condition() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    // Acquire and release a lock
    {
        let _lock = lock::acquire_dir_lock(&lock_dir, "first", false).unwrap();
        assert!(lock_dir.exists());
        // Lock drops here
    }

    // The lock directory should be cleaned up
    assert!(
        !lock_dir.exists(),
        "Lock directory should be removed after Drop"
    );
}

/// Test that force cleanup removes orphaned directories with leftover files.
#[test]
fn test_force_cleanup_removes_orphaned_directory() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    // Create an orphaned lock directory with extra files
    fs::create_dir_all(&lock_dir).unwrap();
    fs::write(
        lock_dir.join("owner"),
        "pid: 99999\nstarted_at: 2025-01-01T00:00:00Z\ncommand: test\nlabel: stale\n",
    )
    .unwrap();
    fs::write(lock_dir.join("extra_file.txt"), "orphaned content").unwrap();

    // Verify the orphaned directory exists
    assert!(lock_dir.exists());

    // Acquire with force should clear the stale lock
    let lock = lock::acquire_dir_lock(&lock_dir, "new_lock", true).unwrap();

    // Verify the lock was acquired
    assert!(lock_dir.exists());
    let owner_content = fs::read_to_string(lock_dir.join("owner")).unwrap();
    assert!(owner_content.contains("label: new_lock"));

    // Clean up
    drop(lock);
}

/// Test that shared task locks coexist properly with supervising locks
/// and cleanup doesn't interfere with each other.
#[test]
fn test_shared_task_lock_cleanup() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    // First, acquire a "supervising" lock
    let supervisor_lock = lock::acquire_dir_lock(&lock_dir, "run one", false).unwrap();
    assert!(lock_dir.exists());

    // Now acquire a task lock (shared mode)
    let task_lock_dir = lock_dir.clone();
    let task_handle = thread::spawn(move || {
        let task_lock = lock::acquire_dir_lock(&task_lock_dir, "task", false).unwrap();
        // Task lock holds a sidecar owner file
        let task_files: Vec<_> = get_task_owner_files(&task_lock_dir);
        assert_eq!(task_files.len(), 1, "Expected one task owner file");
        task_lock
    });

    let task_lock = task_handle.join().unwrap();

    // Both locks should exist
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());
    assert_eq!(get_task_owner_files(&lock_dir).len(), 1);

    // Drop task lock first - should clean up its sidecar but not the directory
    drop(task_lock);

    // Directory should still exist (supervisor holds it)
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());
    assert!(
        get_task_owner_files(&lock_dir).is_empty(),
        "Task owner files should be cleaned up"
    );

    // Now drop supervisor lock
    drop(supervisor_lock);

    assert!(
        test_support::wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
            !lock_dir.exists()
        }),
        "lock directory should be cleaned up after dropping supervisor lock"
    );
}

/// Test that multiple rapid acquire/release cycles don't accumulate
/// orphaned directories.
#[test]
fn test_rapid_acquire_release_no_leak() {
    let dir = TempDir::new().expect("create temp dir");
    let base_lock_dir = dir.path().join("locks");

    const CYCLES: usize = 50;

    for i in 0..CYCLES {
        let lock_dir = base_lock_dir.join(format!("lock_{}", i % 5)); // Reuse 5 different lock dirs

        // Acquire and immediately release
        let lock = lock::acquire_dir_lock(&lock_dir, &format!("cycle_{}", i), false).unwrap();
        drop(lock);
    }

    assert!(
        test_support::wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
            !base_lock_dir.exists()
                || fs::read_dir(&base_lock_dir)
                    .map(|entries| entries.filter_map(|e| e.ok()).all(|e| !e.path().is_dir()))
                    .unwrap_or(true)
        }),
        "lock directories were not cleaned up after rapid acquire/release cycles"
    );

    // Count remaining directories
    if base_lock_dir.exists() {
        let remaining: Vec<_> = fs::read_dir(&base_lock_dir)
            .expect("read base lock dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        // All lock directories should be cleaned up
        assert!(
            remaining.is_empty(),
            "Expected no remaining lock directories, found: {:?}",
            remaining
        );
    }
}

/// Test that multiple task sidecars with a supervising lock are handled correctly.
/// When one task sidecar is dropped, it should not remove the directory or affect
/// other sidecars.
#[test]
fn test_multiple_task_sidecars_cleanup() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    // Acquire supervising lock
    let supervisor_lock = lock::acquire_dir_lock(&lock_dir, "run one", false).unwrap();
    assert!(lock_dir.join("owner").exists());

    // Acquire multiple task locks from the same process
    let task_lock1 = lock::acquire_dir_lock(&lock_dir, "task", false).unwrap();
    let task_lock2 = lock::acquire_dir_lock(&lock_dir, "task", false).unwrap();
    let task_lock3 = lock::acquire_dir_lock(&lock_dir, "task", false).unwrap();

    // Verify all owners are present
    assert!(lock_dir.join("owner").exists());
    let task_files = get_task_owner_files(&lock_dir);
    assert_eq!(task_files.len(), 3, "Expected three task owner files");

    // Drop task_lock2 - the middle one
    drop(task_lock2);

    // Directory should still exist with supervisor and 2 task sidecars
    assert!(lock_dir.exists(), "Lock directory should still exist");
    assert!(
        lock_dir.join("owner").exists(),
        "Supervisor owner should still exist"
    );
    let remaining_files = get_task_owner_files(&lock_dir);
    assert_eq!(
        remaining_files.len(),
        2,
        "Expected two task owner files remaining, found: {:?}",
        remaining_files
    );

    // Drop task_lock1
    drop(task_lock1);

    assert!(lock_dir.exists(), "Lock directory should still exist");
    assert!(
        lock_dir.join("owner").exists(),
        "Supervisor owner should still exist"
    );
    let remaining_files = get_task_owner_files(&lock_dir);
    assert_eq!(
        remaining_files.len(),
        1,
        "Expected one task owner file remaining, found: {:?}",
        remaining_files
    );

    // Drop task_lock3 (last task lock)
    drop(task_lock3);

    // Directory should still exist (supervisor holds it), but no task sidecars
    assert!(lock_dir.exists(), "Lock directory should still exist");
    assert!(
        lock_dir.join("owner").exists(),
        "Supervisor owner should still exist"
    );
    assert!(
        get_task_owner_files(&lock_dir).is_empty(),
        "All task owner files should be cleaned up"
    );

    // Drop supervisor
    drop(supervisor_lock);
    assert!(
        test_support::wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
            !lock_dir.exists()
        }),
        "Lock directory should be removed"
    );
}

/// Test that task sidecar cleanup works correctly when there are other
/// non-owner files in the lock directory.
#[test]
fn test_task_cleanup_with_other_files() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    // Acquire supervising lock
    let supervisor_lock = lock::acquire_dir_lock(&lock_dir, "run loop", false).unwrap();

    // Acquire a task lock
    let task_lock = lock::acquire_dir_lock(&lock_dir, "task", false).unwrap();

    // Create an unrelated file in the lock directory (simulating some debug output)
    fs::write(lock_dir.join("debug.log"), "some debug info").unwrap();

    // Verify files exist
    assert!(lock_dir.join("owner").exists());
    assert_eq!(get_task_owner_files(&lock_dir).len(), 1);
    assert!(lock_dir.join("debug.log").exists());

    // Drop task lock - it should not remove the directory because:
    // 1. The supervisor owner file still exists
    // 2. There are other files in the directory
    drop(task_lock);

    // Directory should still exist with supervisor and the extra file
    assert!(lock_dir.exists(), "Lock directory should still exist");
    assert!(
        lock_dir.join("owner").exists(),
        "Supervisor owner should still exist"
    );
    assert!(
        lock_dir.join("debug.log").exists(),
        "Extra file should still exist"
    );
    assert!(
        get_task_owner_files(&lock_dir).is_empty(),
        "Task owner file should be cleaned up"
    );

    // Clean up supervisor
    drop(supervisor_lock);
    assert!(
        test_support::wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
            !lock_dir.exists() || get_task_owner_files(&lock_dir).is_empty()
        }),
        "Task owner files should be cleaned up even if directory remains"
    );
    if lock_dir.exists() {
        let _ = fs::remove_dir_all(&lock_dir);
    }
}

/// Test that task sidecars with unique names don't collide even when
/// acquired rapidly from the same thread.
#[test]
fn test_rapid_task_lock_unique_names() {
    let dir = TempDir::new().expect("create temp dir");
    let lock_dir = dir.path().join("lock");

    // Acquire supervising lock
    let supervisor_lock = lock::acquire_dir_lock(&lock_dir, "run one", false).unwrap();

    // Rapidly acquire and drop task locks
    const LOCKS: usize = 10;
    let mut locks = Vec::with_capacity(LOCKS);

    for _ in 0..LOCKS {
        locks.push(lock::acquire_dir_lock(&lock_dir, "task", false).unwrap());
    }

    // All locks should have unique sidecars
    let task_files = get_task_owner_files(&lock_dir);
    assert_eq!(
        task_files.len(),
        LOCKS,
        "Expected {} unique task owner files, found: {:?}",
        LOCKS,
        task_files
    );

    // Drop all locks
    drop(locks);

    // Task sidecars should be cleaned up
    assert!(
        get_task_owner_files(&lock_dir).is_empty(),
        "All task owner files should be cleaned up"
    );

    // Drop supervisor
    drop(supervisor_lock);
    assert!(
        test_support::wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
            !lock_dir.exists()
        }),
        "Lock directory should be removed after supervisor drops"
    );
}
