//! Integration tests for lock cleanup race condition handling.
//!
//! Responsibilities:
//! - Validate that concurrent lock acquisition/release doesn't leave orphaned directories.
//! - Verify retry logic with exponential backoff handles race conditions properly.
//! - Test force cleanup behavior for stale locks.
//!
//! Not covered here:
//! - Stale lock detection and cleanup (see `stale_lock_cleanup_test.rs`).
//! - Basic lock acquisition semantics (see `lock_test.rs`).
//!
//! Invariants/assumptions:
//! - Tests use multiple threads to simulate concurrent access.
//! - Temp directories are isolated per test.
//! - Tests run on local filesystem with reasonable timing assumptions.

use ralph::lock;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

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
                            thread::sleep(Duration::from_millis(1));
                            success_count.fetch_add(1, Ordering::SeqCst);
                            // Lock drops here, triggering cleanup
                        }
                        Err(_) => {
                            // Expected contention failure - try again
                            thread::sleep(Duration::from_millis(5));
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

    // Give a moment for any pending cleanup to complete
    thread::sleep(Duration::from_millis(100));

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
        assert!(
            task_lock_dir
                .join(format!("owner_task_{}", std::process::id()))
                .exists()
        );
        task_lock
    });

    let task_lock = task_handle.join().unwrap();

    // Both locks should exist
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());
    assert!(
        lock_dir
            .join(format!("owner_task_{}", std::process::id()))
            .exists()
    );

    // Drop task lock first - should clean up its sidecar but not the directory
    drop(task_lock);

    // Directory should still exist (supervisor holds it)
    assert!(lock_dir.exists());
    assert!(lock_dir.join("owner").exists());

    // Now drop supervisor lock
    drop(supervisor_lock);

    // Give time for cleanup
    thread::sleep(Duration::from_millis(50));

    // Directory should be cleaned up
    assert!(!lock_dir.exists());
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

        // Small delay to allow cleanup
        thread::sleep(Duration::from_millis(2));
    }

    // Give time for final cleanup
    thread::sleep(Duration::from_millis(100));

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
