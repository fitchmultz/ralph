//! Integration test for task split output ID accuracy.
//!
//! Purpose:
//! - Integration test for task split output ID accuracy.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Verifies that the printed child task IDs match the actual inserted IDs,
//! even when higher IDs already exist in the queue.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::{Context, Result};
use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use std::path::Path;

mod test_support;
use test_support::{
    git_init, make_test_task, ralph_init, read_queue, run_in_dir, temp_dir_outside_repo,
    write_queue,
};

fn write_done_empty(dir: &Path) -> Result<()> {
    let done = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let done_path = ralph_dir.join("done.json");
    let json = serde_json::to_string_pretty(&done)?;
    std::fs::write(&done_path, json).context("write done.json")?;
    Ok(())
}

#[test]
fn task_split_output_shows_actual_child_ids() -> Result<()> {
    // Setup: Create temp repo outside of current repo
    let dir = temp_dir_outside_repo();

    // Initialize git and ralph
    git_init(dir.path()).context("git init")?;
    ralph_init(dir.path()).context("ralph init")?;

    // Create a queue with RQ-0001 and RQ-0050 (high ID existing)
    let task_0001 = Task {
        id: "RQ-0001".to_string(),
        title: "Source task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec!["step 1".to_string(), "step 2".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };

    let task_0050 = make_test_task("RQ-0050", "High ID task", TaskStatus::Todo);

    write_queue(dir.path(), &[task_0001, task_0050]).context("write queue")?;
    write_done_empty(dir.path()).context("write empty done")?;

    // Split RQ-0001 into 2 children
    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["task", "split", "--number", "2", "RQ-0001"]);

    anyhow::ensure!(
        status.success(),
        "task split failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify output shows correct child IDs
    // Expected: RQ-0051 and RQ-0052 (not RQ-0002/RQ-0003)
    assert!(
        stdout.contains("Created RQ-0051"),
        "Output should show RQ-0051.\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Created RQ-0052"),
        "Output should show RQ-0052.\nstdout:\n{stdout}"
    );

    // Verify output does NOT show wrong IDs
    assert!(
        !stdout.contains("Created RQ-0002"),
        "Output should NOT show derived RQ-0002.\nstdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("Created RQ-0003"),
        "Output should NOT show derived RQ-0003.\nstdout:\n{stdout}"
    );

    // Verify the queue actually contains the correct child tasks
    let queue = read_queue(dir.path()).context("read queue")?;

    // Find children by parent_id
    let children: Vec<&Task> = queue
        .tasks
        .iter()
        .filter(|t| t.parent_id.as_deref() == Some("RQ-0001"))
        .collect();

    assert_eq!(
        children.len(),
        2,
        "Queue should have exactly 2 children with parent_id RQ-0001"
    );

    let child_ids: Vec<&str> = children.iter().map(|t| t.id.as_str()).collect();
    assert!(
        child_ids.contains(&"RQ-0051"),
        "Queue should contain RQ-0051. Got: {:?}",
        child_ids
    );
    assert!(
        child_ids.contains(&"RQ-0052"),
        "Queue should contain RQ-0052. Got: {:?}",
        child_ids
    );

    Ok(())
}

#[test]
fn task_split_output_with_empty_queue() -> Result<()> {
    // Setup: Create temp repo outside of current repo
    let dir = temp_dir_outside_repo();

    // Initialize git and ralph
    git_init(dir.path()).context("git init")?;
    ralph_init(dir.path()).context("ralph init")?;

    // Create a queue with only RQ-0001
    let task_0001 = Task {
        id: "RQ-0001".to_string(),
        title: "Source task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };

    write_queue(dir.path(), &[task_0001]).context("write queue")?;
    write_done_empty(dir.path()).context("write empty done")?;

    // Split RQ-0001 into 2 children
    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["task", "split", "--number", "2", "RQ-0001"]);

    anyhow::ensure!(
        status.success(),
        "task split failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // With empty queue, IDs should be sequential starting from RQ-0002
    assert!(
        stdout.contains("Created RQ-0002"),
        "Output should show RQ-0002.\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Created RQ-0003"),
        "Output should show RQ-0003.\nstdout:\n{stdout}"
    );

    // Verify the queue actually contains the correct child tasks
    let queue = read_queue(dir.path()).context("read queue")?;

    // Find children by parent_id
    let children: Vec<&Task> = queue
        .tasks
        .iter()
        .filter(|t| t.parent_id.as_deref() == Some("RQ-0001"))
        .collect();

    assert_eq!(
        children.len(),
        2,
        "Queue should have exactly 2 children with parent_id RQ-0001"
    );

    let child_ids: Vec<&str> = children.iter().map(|t| t.id.as_str()).collect();
    assert!(
        child_ids.contains(&"RQ-0002"),
        "Queue should contain RQ-0002. Got: {:?}",
        child_ids
    );
    assert!(
        child_ids.contains(&"RQ-0003"),
        "Queue should contain RQ-0003. Got: {:?}",
        child_ids
    );

    Ok(())
}
