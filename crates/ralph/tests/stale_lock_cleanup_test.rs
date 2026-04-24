//! Integration tests for stale lock cleanup behavior.
//!
//! Purpose:
//! - Integration tests for stale lock cleanup behavior.
//!
//! Responsibilities:
//! - Verify force acquisition clears stale lock metadata.
//! - Ensure active locks are not removed even when forced.
//! - Verify resume flow clears stale locks automatically (regression test for RQ-0643).
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not covered here:
//! - Shared task lock behavior (see `task_lock_coexistence_test.rs`).
//! - Temp directory helpers or atomic writes.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The fake PID used is not running on the test system.
//! - Error messages retain the "stale pid" signal.

use anyhow::{Context, Result};
use ralph::lock;
use std::fs;
use tempfile::TempDir;

#[cfg(unix)]
mod lock_support;

#[cfg(unix)]
#[test]
fn acquire_dir_lock_auto_cleans_stale_lock_when_forced() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let lock_dir = dir.path().join("lock");

    // Create a "stale" lock by manually creating the directory and an owner file with a non-existent PID.
    fs::create_dir_all(&lock_dir)?;
    let owner_path = lock_dir.join("owner");

    let stale_pid = lock_support::spawn_exited_pid();
    fs::write(
        &owner_path,
        format!(
            "pid: {stale_pid}\nstarted_at: 2026-01-18T00:00:00Z\ncommand: old-ralph\nlabel: stale-lock\n"
        ),
    )?;

    // Attempting to acquire without force should fail.
    let err = lock::acquire_dir_lock(&lock_dir, "new-proc", false).unwrap_err();
    assert!(format!("{err:#}").to_lowercase().contains("stale pid"));

    // Attempting to acquire with force should succeed.
    let _lock =
        lock::acquire_dir_lock(&lock_dir, "new-proc", true).context("acquire with force")?;
    assert!(lock_dir.exists());
    assert!(owner_path.exists());

    let owner_content = fs::read_to_string(&owner_path)?;
    assert!(owner_content.contains("new-proc"));
    assert!(owner_content.contains(&std::process::id().to_string()));

    Ok(())
}

#[test]
fn acquire_dir_lock_does_not_clean_active_lock_even_if_forced() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let lock_dir = dir.path().join("lock");

    // Create a lock with the current PID (active).
    let _lock1 = lock::acquire_dir_lock(&lock_dir, "proc1", false).context("acquire first lock")?;

    // Attempting to acquire with force should still fail because the PID is active.
    let err = lock::acquire_dir_lock(&lock_dir, "proc2", true).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.to_lowercase().contains("lock already held"));
    assert!(msg.to_lowercase().contains("pid:"));
    assert!(!msg.to_lowercase().contains("stale pid"));

    Ok(())
}

/// Regression test for RQ-0643: force=true clears stale queue lock.
///
/// This test verifies the underlying mechanism used by the resume flow:
/// acquire_queue_lock with force=true clears stale locks (PIDs that are dead).
/// The run_loop calls this during resume to clear any stale locks left by
/// a crashed or killed ralph process.
#[cfg(unix)]
#[test]
fn acquire_dir_lock_with_force_clears_stale_lock_for_resume() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();
    fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    // Create a stale queue lock with a dead PID
    let lock_dir = lock::queue_lock_dir(&repo_root);
    fs::create_dir_all(&lock_dir)?;
    let stale_pid = lock_support::spawn_exited_pid();
    fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {stale_pid}\nstarted_at: 2026-02-06T00:56:29Z\ncommand: ralph run loop --max-tasks 0\nlabel: run one\n"
        ),
    )?;

    // Verify the lock exists and appears stale
    assert!(lock_dir.exists(), "lock dir should exist");
    let err = lock::acquire_dir_lock(&lock_dir, "test", false).unwrap_err();
    assert!(
        format!("{err:#}").contains("STALE PID"),
        "expected stale PID error"
    );

    // Acquire with force=true should succeed and clear the stale lock
    let lock = lock::acquire_dir_lock(&lock_dir, "run loop resume", true)?;
    drop(lock);

    // After dropping, the lock should be fully cleaned up.
    assert!(
        !lock_dir.exists(),
        "expected lock dir to be removed on drop"
    );

    Ok(())
}
