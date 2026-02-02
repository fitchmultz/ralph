//! Integration tests for `ralph queue repair`.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

mod test_support;

fn ralph_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ralph") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");
    let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        exe_dir
            .parent()
            .expect("deps directory should have a parent directory")
    } else {
        exe_dir
    };

    let bin_name = if cfg!(windows) { "ralph.exe" } else { "ralph" };
    let candidate = profile_dir.join(bin_name);
    if candidate.exists() {
        return candidate;
    }

    panic!(
        "CARGO_BIN_EXE_ralph was not set and fallback binary path does not exist: {}",
        candidate.display()
    );
}

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(ralph_bin())
        .current_dir(dir)
        .env_remove("RUST_LOG")
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn repair_queue_fixes_missing_fields_and_duplicates() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Create broken queue.json
    // - RQ-0001: Missing request, missing created_at/updated_at, empty title
    // - RQ-0001: Duplicate ID
    let broken_queue = r#"{ 
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "",
      "tags": [],
      "scope": [],
      "evidence": [],
      "plan": [],
      "notes": [],
      "depends_on": [],
      "custom_fields": {}
    },
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Duplicate task",
      "tags": ["rust"],
      "scope": ["crates/ralph"],
      "evidence": ["none"],
      "plan": ["none"],
      "request": "Some request",
      "created_at": "2026-01-18T00:00:00.000000000Z",
      "updated_at": "2026-01-18T00:00:00.000000000Z",
      "completed_at": null,
      "notes": [],
      "depends_on": [],
      "custom_fields": {}
    }
  ]
}"#;

    // Create broken done.json
    // - RQ-0002: Valid
    // - RQ-0001: Duplicate from queue
    let broken_done = r#"{ 
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0002",
      "status": "done",
      "title": "Valid done task",
      "tags": [],
      "scope": [],
      "evidence": ["ok"],
      "plan": ["ok"],
      "request": "done",
      "created_at": "2026-01-18T00:00:00.000000000Z",
      "updated_at": "2026-01-18T00:00:00.000000000Z",
      "completed_at": "2026-01-18T00:00:00.000000000Z",
      "notes": [],
      "depends_on": [],
      "custom_fields": {}
    },
    {
      "id": "RQ-0001",
      "status": "done",
      "title": "Duplicate done task",
      "tags": [],
      "scope": [],
      "evidence": ["ok"],
      "plan": ["ok"],
      "request": "done",
      "created_at": "2026-01-18T00:00:00.000000000Z",
      "updated_at": "2026-01-18T00:00:00.000000000Z",
      "completed_at": "2026-01-18T00:00:00.000000000Z",
      "notes": [],
      "depends_on": [],
      "custom_fields": {}
    }
  ]
}"#;

    std::fs::write(dir.path().join(".ralph/queue.json"), broken_queue)?;
    std::fs::write(dir.path().join(".ralph/done.json"), broken_done)?;

    // Run repair
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "repair"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue repair failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    println!("Stdout: {stdout}");
    println!("Stderr: {stderr}");

    // Check stderr for report (log::info! goes to stderr)
    assert!(stderr.contains("Fixed missing fields"));
    assert!(stderr.contains("Fixed invalid timestamps")); // RQ-0001 missing timestamps
    assert!(stderr.contains("Remapped"));
    assert!(stderr.contains("Repaired queue written to disk"));

    // Verify file content
    let queue_path = dir.path().join(".ralph/queue.json");
    let done_path = dir.path().join(".ralph/done.json");

    let queue_str = std::fs::read_to_string(&queue_path)?;
    let done_str = std::fs::read_to_string(&done_path)?;

    // Verify duplicate IDs are gone
    // IDs in queue should be RQ-0001 and RQ-0003 (since RQ-0002 is in done)
    // Or maybe different depending on iteration order.
    //
    // Logic:
    // 1. Scan active: RQ-0001, RQ-0001.
    // 2. Scan done: RQ-0002, RQ-0001.
    // Max ID seen is RQ-0002. Next is RQ-0003.
    //
    // Processing Active:
    // - Task 1 (RQ-0001): kept as RQ-0001.
    // - Task 2 (RQ-0001): duplicate -> remapped to RQ-0003. Next is RQ-0004.
    //
    // Processing Done:
    // - Task 1 (RQ-0002): kept as RQ-0002.
    // - Task 2 (RQ-0001): duplicate (seen in active) -> remapped to RQ-0004.

    // So we expect:
    // Queue: RQ-0001, RQ-0003
    // Done: RQ-0002, RQ-0004

    assert!(
        queue_str.contains("RQ-0001"),
        "Queue should contain RQ-0001"
    );
    assert!(
        queue_str.contains("RQ-0003"),
        "Queue should contain RQ-0003"
    );
    assert!(
        !queue_str.contains(
            "\"id\": \"RQ-0001\",\n      \"status\": \"todo\",\n      \"title\": \"Duplicate task\""
        ),
        "Duplicate task should be renamed"
    );

    assert!(done_str.contains("RQ-0002"), "Done should contain RQ-0002");
    assert!(
        done_str.contains("RQ-0004"),
        "Done should contain RQ-0004 (remapped)"
    );
    assert!(
        !done_str.contains("\"id\": \"RQ-0001\""),
        "Done should not contain RQ-0001"
    );

    // Verify fields fixed
    assert!(
        queue_str.contains("Untitled"),
        "Task 1 should have title Untitled"
    );
    assert!(
        queue_str.contains("Imported task"),
        "Task 1 should have request Imported task"
    );
    // We can't easily regex timestamps but we know they are there if JSON is valid and parsing passed.
    Ok(())
}

#[test]
fn repair_remaps_dependencies_for_invalid_ids() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Create broken queue.json
    // - INVALID-1: Invalid ID format
    // - RQ-0002: Depends on INVALID-1
    let broken_queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "INVALID-1",
      "status": "todo",
      "title": "Invalid ID task",
      "tags": ["test"],
      "scope": ["crates/ralph"],
      "evidence": ["none"],
      "plan": ["none"],
      "request": "Test request",
      "created_at": "2026-01-18T00:00:00.000000000Z",
      "updated_at": "2026-01-18T00:00:00.000000000Z",
      "completed_at": null,
      "notes": [],
      "depends_on": [],
      "custom_fields": {}
    },
    {
      "id": "RQ-0002",
      "status": "todo",
      "title": "Dependent task",
      "tags": ["test"],
      "scope": ["crates/ralph"],
      "evidence": ["none"],
      "plan": ["none"],
      "request": "Test request",
      "created_at": "2026-01-18T00:00:00.000000000Z",
      "updated_at": "2026-01-18T00:00:00.000000000Z",
      "completed_at": null,
      "notes": [],
      "depends_on": ["INVALID-1"],
      "custom_fields": {}
    }
  ]
}"#;

    std::fs::write(dir.path().join(".ralph/queue.json"), broken_queue)?;

    // Run repair
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "repair"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue repair failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue_str = std::fs::read_to_string(dir.path().join(".ralph/queue.json"))?;

    // INVALID-1 should be remapped to RQ-0001 (since init likely created nothing or we overwrote it)
    // Actually init creates nothing in queue.json usually, just structure.
    // The test overwrote queue.json, so next available valid ID should be RQ-0001 or RQ-0003 depending on what RQ-0002 occupies.
    // RQ-0002 is valid.
    // So INVALID-1 should become RQ-0001 (or RQ-0003 if it scans and sees RQ-0002).

    // Let's verify that INVALID-1 is GONE and replaced by a valid ID.
    assert!(
        !queue_str.contains("INVALID-1"),
        "INVALID-1 should be remapped"
    );

    // Find the new ID for the first task
    let queue: serde_json::Value = serde_json::from_str(&queue_str)?;
    let tasks = queue["tasks"].as_array().expect("tasks array");

    let task1 = tasks
        .iter()
        .find(|t| t["title"] == "Invalid ID task")
        .expect("Task 1 found");
    let new_id = task1["id"].as_str().expect("id string");

    println!("Remapped ID: {}", new_id);
    assert!(new_id.starts_with("RQ-"), "New ID should start with RQ-");

    // Verify dependent task points to new ID
    let task2 = tasks
        .iter()
        .find(|t| t["title"] == "Dependent task")
        .expect("Task 2 found");
    let depends_on = task2["depends_on"].as_array().expect("depends_on array");

    assert_eq!(depends_on.len(), 1, "Should have 1 dependency");
    assert_eq!(
        depends_on[0].as_str(),
        Some(new_id),
        "Dependency should be updated to new ID"
    );

    Ok(())
}
