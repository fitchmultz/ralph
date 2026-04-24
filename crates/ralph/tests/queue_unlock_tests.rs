//! Integration tests for `ralph queue unlock` safety features.
//!
//! Purpose:
//! - Integration tests for `ralph queue unlock` safety features.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Result;
use std::fs;
use std::path::Path;

mod test_support;

fn create_lock_with_pid(dir: &Path, pid: u32) -> Result<()> {
    let lock_dir = dir.join(".ralph").join("lock");
    fs::create_dir_all(&lock_dir)?;
    let owner_path = lock_dir.join("owner");
    let content = format!(
        "pid: {}\nstarted_at: 2026-01-01T00:00:00Z\ncommand: test\nlabel: test\n",
        pid
    );
    fs::write(&owner_path, content)?;
    Ok(())
}

#[test]
fn test_unlock_dry_run_shows_lock_info() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create lock with current process PID (definitely running)
    let current_pid = std::process::id();
    create_lock_with_pid(dir.path(), current_pid)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "unlock", "--dry-run"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(
        combined.contains("dry-run") || combined.contains("Dry run"),
        "Expected dry-run message: {}",
        combined
    );

    // Lock should still exist
    let lock_dir = dir.path().join(".ralph").join("lock");
    assert!(
        lock_dir.exists(),
        "Lock should not be removed in dry-run mode"
    );

    Ok(())
}

#[test]
fn test_unlock_blocked_for_active_process() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create lock with current process PID (definitely running)
    let current_pid = std::process::id();
    create_lock_with_pid(dir.path(), current_pid)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "unlock", "--yes"]); // No --force

    // Should fail because process is running
    assert!(!status.success(), "Expected failure for active process");
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(
        combined.contains("force") || combined.contains("running"),
        "Should mention --force or running process: {}",
        combined
    );

    // Lock should still exist
    let lock_dir = dir.path().join(".ralph").join("lock");
    assert!(lock_dir.exists(), "Lock should not be removed");

    Ok(())
}

#[test]
fn test_unlock_succeeds_with_force() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create lock with current process PID
    let current_pid = std::process::id();
    create_lock_with_pid(dir.path(), current_pid)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "unlock", "--force", "--yes"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);

    // Lock should be removed
    let lock_dir = dir.path().join(".ralph").join("lock");
    assert!(
        !lock_dir.exists(),
        "Lock should be removed with --force --yes"
    );

    Ok(())
}

#[test]
fn test_unlock_succeeds_for_dead_process() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create lock with a PID that definitely doesn't exist (max value)
    create_lock_with_pid(dir.path(), 0xFFFFFFFE)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "unlock", "--yes"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);

    // Lock should be removed
    let lock_dir = dir.path().join(".ralph").join("lock");
    assert!(
        !lock_dir.exists(),
        "Lock should be removed for dead process"
    );

    Ok(())
}

#[test]
fn test_unlock_no_lock_exists() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["queue", "unlock"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(
        combined.contains("not locked") || combined.contains("no lock"),
        "Should indicate queue is not locked: {}",
        combined
    );

    Ok(())
}

#[test]
fn test_unlock_help_shows_new_options() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "unlock", "--help"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Check for new options
    assert!(
        combined.contains("--force"),
        "Help should mention --force: {}",
        combined
    );
    assert!(
        combined.contains("--yes"),
        "Help should mention --yes: {}",
        combined
    );
    assert!(
        combined.contains("--dry-run"),
        "Help should mention --dry-run: {}",
        combined
    );

    Ok(())
}

#[test]
fn test_unlock_dry_run_shows_process_status() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create lock with current process PID (definitely running)
    let current_pid = std::process::id();
    create_lock_with_pid(dir.path(), current_pid)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "unlock", "--dry-run"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Should show the process is running
    assert!(
        combined.contains("RUNNING") || combined.contains("running"),
        "Should show process status: {}",
        combined
    );

    Ok(())
}

#[test]
fn test_unlock_malformed_owner_file() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create lock with malformed owner file
    let lock_dir = dir.path().join(".ralph").join("lock");
    fs::create_dir_all(&lock_dir)?;
    fs::write(lock_dir.join("owner"), "garbage: data\nno valid pid here")?;

    let (status, _, _) = test_support::run_in_dir(dir.path(), &["queue", "unlock", "--yes"]);

    assert!(status.success(), "Should succeed for malformed owner file");
    assert!(
        !lock_dir.exists(),
        "Lock should be removed for malformed owner"
    );

    Ok(())
}

#[test]
fn test_unlock_missing_owner_file() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create lock directory but no owner file
    let lock_dir = dir.path().join(".ralph").join("lock");
    fs::create_dir_all(&lock_dir)?;

    let (status, _, _) = test_support::run_in_dir(dir.path(), &["queue", "unlock", "--yes"]);

    assert!(status.success(), "Should succeed when no owner file");
    assert!(
        !lock_dir.exists(),
        "Lock should be removed when no owner file"
    );

    Ok(())
}

#[test]
fn test_unlock_dry_run_shows_not_running_status() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Create lock with a PID that definitely doesn't exist
    create_lock_with_pid(dir.path(), 0xFFFFFFFE)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "unlock", "--dry-run"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Should show the process is NOT RUNNING
    assert!(
        combined.contains("NOT RUNNING") || combined.contains("safe to unlock"),
        "Should show NOT RUNNING status for dead PID: {}",
        combined
    );

    Ok(())
}
