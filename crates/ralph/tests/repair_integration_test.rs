//! Purpose: Exercise persisted `ralph queue repair` behavior.
//!
//! Responsibilities:
//! - Verify CLI repair rewrites queue and done files safely.
//! - Cover regressions that require full on-disk repair and validation flows.
//!
//! Scope:
//! - Integration coverage for the `ralph queue repair` command.
//! - Unit-level repair helper behavior belongs in `crates/ralph/src/queue/repair.rs`.
//!
//! Usage:
//! - Run through Cargo integration tests for the `ralph` crate.
//!
//! Invariants/Assumptions:
//! - Test workspaces are created outside the repository to avoid nested repo detection.
//! - Each scenario initializes its own Ralph workspace before replacing fixtures.

use anyhow::Result;
use std::path::Path;
use std::process::ExitStatus;

mod test_support;

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
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

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), broken_queue)?;
    std::fs::write(dir.path().join(".ralph/done.jsonc"), broken_done)?;

    // Run repair
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "repair"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue repair failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Queue repair now narrates continuation guidance on stdout.
    assert!(stdout.contains("Queue continuation has been normalized."));
    assert!(stdout.contains("\"fixed_tasks\": 3"));
    assert!(stdout.contains("\"fixed_timestamps\": 2"));
    assert!(stdout.contains("\"remapped_ids\""));
    assert!(stdout.contains("ralph queue validate"));

    // Verify file content
    let queue_path = dir.path().join(".ralph/queue.jsonc");
    let done_path = dir.path().join(".ralph/done.jsonc");

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
fn repair_remaps_all_relationship_fields_for_invalid_ids() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Create broken queue.json:
    // - INVALID-1: Invalid ID format.
    // - RQ-0002: References INVALID-1 through every task-ID relationship field.
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
      "blocks": [],
      "relates_to": [],
      "custom_fields": {}
    },
    {
      "id": "RQ-0002",
      "status": "draft",
      "title": "Relationship task",
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
      "blocks": ["INVALID-1"],
      "relates_to": ["INVALID-1"],
      "duplicates": "INVALID-1",
      "custom_fields": {},
      "parent_id": "INVALID-1"
    }
  ]
}"#;

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), broken_queue)?;

    // Run repair
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "repair"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue repair failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue_str = std::fs::read_to_string(dir.path().join(".ralph/queue.jsonc"))?;

    // Verify that INVALID-1 is gone and replaced by a valid generated ID.
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

    assert!(new_id.starts_with("RQ-"), "New ID should start with RQ-");

    // Verify the referencing task points to the remapped ID everywhere.
    let task2 = tasks
        .iter()
        .find(|t| t["title"] == "Relationship task")
        .expect("Task 2 found");
    assert_single_id(task2, "depends_on", new_id);
    assert_single_id(task2, "blocks", new_id);
    assert_single_id(task2, "relates_to", new_id);
    assert_eq!(task2["duplicates"].as_str(), Some(new_id));
    assert_eq!(task2["parent_id"].as_str(), Some(new_id));

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "validate"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue validate failed after repair\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

fn assert_single_id(task: &serde_json::Value, field: &str, expected_id: &str) {
    let values = task[field].as_array().unwrap_or_else(|| {
        panic!("{field} should be an array");
    });

    assert_eq!(values.len(), 1, "{field} should have 1 ID");
    assert_eq!(
        values[0].as_str(),
        Some(expected_id),
        "{field} should be updated to the remapped ID"
    );
}
