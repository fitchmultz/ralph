//! Integration tests for `ralph machine queue unlock-inspect`.

use anyhow::Result;
use serde_json::Value;
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
fn machine_queue_unlock_inspect_reports_clear_when_no_lock_exists() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["machine", "queue", "unlock-inspect"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);
    let json: Value = serde_json::from_str(&stdout)?;
    assert_eq!(json["version"], 1);
    assert_eq!(json["condition"], "clear");
    assert_eq!(json["unlock_allowed"], false);
    Ok(())
}

#[test]
fn machine_queue_unlock_inspect_reports_stale_lock() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;
    create_lock_with_pid(dir.path(), 0xFFFFFFFE)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["machine", "queue", "unlock-inspect"]);

    assert!(status.success(), "stdout:\n{}\nstderr:\n{}", stdout, stderr);
    let json: Value = serde_json::from_str(&stdout)?;
    assert_eq!(json["version"], 1);
    assert_eq!(json["condition"], "stale");
    assert_eq!(json["unlock_allowed"], true);
    assert_eq!(json["blocking"]["reason"]["kind"], "lock_blocked");
    Ok(())
}
