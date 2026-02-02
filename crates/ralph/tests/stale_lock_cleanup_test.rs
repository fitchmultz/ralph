//! Integration tests for stale lock cleanup behavior.
//!
//! Responsibilities:
//! - Verify force acquisition clears stale lock metadata.
//! - Ensure active locks are not removed even when forced.
//!
//! Not covered here:
//! - Shared task lock behavior (see `task_lock_coexistence_test.rs`).
//! - Temp directory helpers or atomic writes.
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
