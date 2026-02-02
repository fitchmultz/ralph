//! Tests for supervising-process detection of TUI lock owners.
//!
//! Responsibilities:
//! - Ensure supervising labels are detected from lock metadata.
//! - Validate TUI label handling in lock ownership parsing.
//!
//! Not covered here:
//! - Lock acquisition semantics (see `lock_test.rs`).
//! - CLI or queue workflows.
//!
//! Invariants/assumptions:
//! - Lock owner file is a simple key/value text file.
//! - Labels are compared using the supervising label list.

use anyhow::Result;
use ralph::lock;
use tempfile::TempDir;

#[test]
fn supervising_process_detects_tui_label() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let lock_dir = temp_dir.path().join("lock");
    std::fs::create_dir_all(&lock_dir)?;
    let owner_path = lock_dir.join("owner");
    let owner = "pid: 123\nstarted_at: now\ncommand: ralph tui\nlabel: tui\n";
    std::fs::write(&owner_path, owner)?;

    assert!(lock::is_supervising_process(&lock_dir)?);
    Ok(())
}
