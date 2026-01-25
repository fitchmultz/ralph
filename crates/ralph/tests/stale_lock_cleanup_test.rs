//! Integration tests for stale lock cleanup behavior.

use anyhow::{Context, Result};
use ralph::fsutil;
use std::fs;
use tempfile::TempDir;

#[test]
fn acquire_dir_lock_auto_cleans_stale_lock_when_forced() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let lock_dir = dir.path().join("lock");

    // Create a "stale" lock by manually creating the directory and an owner file with a non-existent PID.
    fs::create_dir_all(&lock_dir)?;
    let owner_path = lock_dir.join("owner");

    // Use a PID that is highly unlikely to be running.
    // On most systems, PIDs are up to 32768 or 65536. 999999 is usually safe.
    fs::write(
        &owner_path,
        "pid: 999999\nstarted_at: 2026-01-18T00:00:00Z\ncommand: old-ralph\nlabel: stale-lock\n",
    )?;

    // Attempting to acquire without force should fail.
    let err = fsutil::acquire_dir_lock(&lock_dir, "new-proc", false).unwrap_err();
    assert!(format!("{err:#}").to_lowercase().contains("stale pid"));

    // Attempting to acquire with force should succeed.
    let _lock =
        fsutil::acquire_dir_lock(&lock_dir, "new-proc", true).context("acquire with force")?;
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
    let _lock1 =
        fsutil::acquire_dir_lock(&lock_dir, "proc1", false).context("acquire first lock")?;

    // Attempting to acquire with force should still fail because the PID is active.
    let err = fsutil::acquire_dir_lock(&lock_dir, "proc2", true).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.to_lowercase().contains("lock already held"));
    assert!(msg.to_lowercase().contains("pid:"));
    assert!(!msg.to_lowercase().contains("stale pid"));

    Ok(())
}
