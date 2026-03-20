//! Purpose: subprocess-based queue lock contention coverage.
//!
//! Responsibilities:
//! - Verify a second process cannot acquire the queue lock while a lock-holder subprocess is active.
//! - Verify supervisor contention errors preserve lock-path and label context.
//!
//! Scope:
//! - Contention scenarios that depend on the self-exec lock-holder helper.
//!
//! Usage:
//! - Tests use `support::spawn_lock_holder()` to preserve the original subprocess setup flow.
//!
//! Invariants/Assumptions:
//! - Test function names, assertions, env vars, and serial semantics remain unchanged.
//! - The readiness contract is still `LOCK_HELD` on stdout.

use super::*;

#[test]
#[serial]
fn lock_contention_blocks_second_process() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();

    let lock_holder = support::spawn_lock_holder(&repo_root, None)?;

    let err = queue::acquire_queue_lock(&repo_root, "contender", false).unwrap_err();
    let msg = format!("{err:#}");
    let lock_dir = lock::queue_lock_dir(&repo_root);

    anyhow::ensure!(
        msg.contains(lock_dir.to_string_lossy().as_ref()),
        "expected lock path in error: {msg}"
    );

    drop(lock_holder);

    Ok(())
}

/// Test that a parallel run loop prevents another run loop from starting.
///
/// This validates the concurrency contract: the queue lock is held for the
/// entire parallel run loop duration, preventing duplicate task selection.
#[test]
#[serial]
fn parallel_supervisor_prevents_second_supervisor() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();

    let lock_holder = support::spawn_lock_holder(&repo_root, Some("run loop"))?;

    let err = queue::acquire_queue_lock(&repo_root, "run loop", false).unwrap_err();
    let msg = format!("{err:#}");
    let lock_dir = lock::queue_lock_dir(&repo_root);

    anyhow::ensure!(
        msg.contains(lock_dir.to_string_lossy().as_ref()),
        "expected lock path in error: {msg}"
    );

    anyhow::ensure!(
        msg.contains("run loop") || msg.contains("already held"),
        "expected 'run loop' or 'already held' in error: {msg}"
    );

    drop(lock_holder);

    Ok(())
}
