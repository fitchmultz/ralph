//! Integration tests for structured task mutation transactions.
//!
//! Responsibilities:
//! - Verify multi-field task mutations apply atomically through the CLI.
//! - Ensure optimistic-lock conflicts do not partially persist queue changes.
//! - Validate bulk status-to-doing updates set `started_at` through the shared mutation path.

mod test_support;

use anyhow::Result;
use ralph::contracts::TaskStatus;
use serde_json::json;
use std::fs;

fn find_task<'a>(
    tasks: &'a [ralph::contracts::Task],
    id: &str,
) -> Option<&'a ralph::contracts::Task> {
    tasks.iter().find(|task| task.id == id)
}

#[test]
fn task_mutate_applies_multi_field_edit_atomically() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let task = test_support::make_test_task("RQ-0001", "Original title", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;

    let request = json!({
        "version": 1,
        "atomic": true,
        "tasks": [{
            "task_id": "RQ-0001",
            "expected_updated_at": "2026-01-19T00:00:00Z",
            "edits": [
                { "field": "title", "value": "Updated title" },
                { "field": "priority", "value": "high" },
                { "field": "description", "value": "Updated description" }
            ]
        }]
    });
    let request_path = dir.path().join("mutation.json");
    fs::write(&request_path, serde_json::to_vec_pretty(&request)?)?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "mutate", "--input", request_path.to_str().unwrap()],
    );
    anyhow::ensure!(
        status.success(),
        "task mutate failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, "RQ-0001").expect("task should exist");
    assert_eq!(task.title, "Updated title");
    assert_eq!(task.priority, ralph::contracts::TaskPriority::High);
    assert_eq!(task.description.as_deref(), Some("Updated description"));
    assert!(stdout.contains("\"applied_edits\": 3"));

    Ok(())
}

#[test]
fn task_mutate_conflict_leaves_task_unchanged() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let task = test_support::make_test_task("RQ-0001", "Original title", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;

    let request = json!({
        "version": 1,
        "atomic": true,
        "tasks": [{
            "task_id": "RQ-0001",
            "expected_updated_at": "2026-01-20T00:00:00Z",
            "edits": [
                { "field": "title", "value": "Should not persist" },
                { "field": "priority", "value": "high" }
            ]
        }]
    });
    let request_path = dir.path().join("conflict.json");
    fs::write(&request_path, serde_json::to_vec_pretty(&request)?)?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "mutate", "--input", request_path.to_str().unwrap()],
    );
    anyhow::ensure!(!status.success(), "task mutate unexpectedly succeeded");
    assert!(stderr.contains("Task mutation conflict"));

    let queue = test_support::read_queue(dir.path())?;
    let task = find_task(&queue.tasks, "RQ-0001").expect("task should exist");
    assert_eq!(task.title, "Original title");
    assert_eq!(task.priority, ralph::contracts::TaskPriority::Medium);

    Ok(())
}

#[test]
fn task_mutate_bulk_doing_sets_started_at() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let first = test_support::make_test_task("RQ-0001", "First", TaskStatus::Todo);
    let second = test_support::make_test_task("RQ-0002", "Second", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[first, second])?;

    let request = json!({
        "version": 1,
        "atomic": true,
        "tasks": [
            {
                "task_id": "RQ-0001",
                "expected_updated_at": "2026-01-19T00:00:00Z",
                "edits": [{ "field": "status", "value": "doing" }]
            },
            {
                "task_id": "RQ-0002",
                "expected_updated_at": "2026-01-19T00:00:00Z",
                "edits": [{ "field": "status", "value": "doing" }]
            }
        ]
    });
    let request_path = dir.path().join("bulk-status.json");
    fs::write(&request_path, serde_json::to_vec_pretty(&request)?)?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "mutate", "--input", request_path.to_str().unwrap()],
    );
    anyhow::ensure!(
        status.success(),
        "task mutate failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue = test_support::read_queue(dir.path())?;
    for task_id in ["RQ-0001", "RQ-0002"] {
        let task = find_task(&queue.tasks, task_id).expect("task should exist");
        assert_eq!(task.status, TaskStatus::Doing);
        assert!(
            task.started_at.is_some(),
            "started_at should be set for {task_id}"
        );
    }

    Ok(())
}
