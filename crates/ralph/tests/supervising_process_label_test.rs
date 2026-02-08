//! Tests for supervising-process detection from lock metadata.
//!
//! Responsibilities:
//! - Ensure supervising labels are detected from lock owner metadata.
//!
//! Not covered here:
//! - Lock acquisition semantics (see `lock_test.rs` and `task_lock_coexistence_test.rs`).
//! - CLI workflows that create or clear locks.
//!
//! Invariants/assumptions:
//! - Lock owner file is a simple key/value text file.
//! - Labels are compared using the supervising label list (e.g., `run one`, `run loop`).

use anyhow::Result;
use ralph::lock;
use tempfile::TempDir;

#[test]
fn supervising_process_detects_run_loop_label() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let lock_dir = temp_dir.path().join("lock");
    std::fs::create_dir_all(&lock_dir)?;
    let owner_path = lock_dir.join("owner");
    let owner = "pid: 123\nstarted_at: now\ncommand: ralph run loop\nlabel: run loop\n";
    std::fs::write(&owner_path, owner)?;

    assert!(lock::is_supervising_process(&lock_dir)?);
    Ok(())
}
