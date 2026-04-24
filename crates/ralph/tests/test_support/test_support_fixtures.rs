//! Queue and task fixture helpers for integration tests.
//!
//! Purpose:
//! - Queue and task fixture helpers for integration tests.
//!
//! Responsibilities:
//! - Build realistic queue/task fixtures for CLI and rendering tests.
//! - Read and write queue/done files under temporary Ralph repos.
//! - Snapshot queue state for isolation assertions.
//!
//! Non-scope:
//! - Repo initialization, command execution, or snapshot formatting.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Queue fixtures target the v1 queue contract.
//! - Snapshot helpers compare raw JSON strings, not semantic task equality.

use anyhow::{Context, Result};
use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};

/// Helper to create a test task.
///
/// The fields are intentionally fully-populated so contract/rendering tests can rely on realistic
/// data without repeating boilerplate.
pub fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    let completed_at = match status {
        TaskStatus::Done | TaskStatus::Rejected => Some("2026-01-19T00:00:00Z".to_string()),
        _ => None,
    };
    Task {
        id: id.to_string(),
        title: title.to_string(),
        description: None,
        status,
        priority: TaskPriority::Medium,
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test evidence".to_string()],
        plan: vec!["test plan".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at,
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
    }
}

/// Helper to create a test queue with multiple tasks.
pub fn make_test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![
            make_test_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_test_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_test_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}

/// Rendering-focused task fixture.
pub fn make_render_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    let mut task = make_test_task(id, title, status);
    task.plan = vec![
        "test plan step 1".to_string(),
        "test plan step 2".to_string(),
    ];
    task.completed_at = None;
    task
}

/// Rendering-focused queue fixture (uses `make_render_test_task`).
pub fn make_render_test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![
            make_render_test_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_render_test_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_render_test_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}

/// Write `.ralph/cache/execution_history.json` with a single v1 entry.
pub fn write_execution_history_v1_single_sample(
    dir: &std::path::Path,
    runner: &str,
    model: &str,
    total_secs: u64,
    planning_secs: u64,
    implementation_secs: u64,
    review_secs: u64,
) -> Result<()> {
    let history = serde_json::json!({
      "version": 1,
      "entries": [
        {
          "timestamp": "2026-02-01T00:00:00Z",
          "task_id": "RQ-9999",
          "runner": runner,
          "model": model,
          "phase_count": 3,
          "phase_durations": {
            "planning": { "secs": planning_secs, "nanos": 0 },
            "implementation": { "secs": implementation_secs, "nanos": 0 },
            "review": { "secs": review_secs, "nanos": 0 }
          },
          "total_duration": { "secs": total_secs, "nanos": 0 }
        }
      ]
    });

    let cache_dir = dir.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir).context("create .ralph/cache")?;
    std::fs::write(
        cache_dir.join("execution_history.json"),
        serde_json::to_string_pretty(&history).context("serialize execution_history.json")?,
    )
    .context("write execution_history.json")?;
    Ok(())
}

pub fn write_valid_single_todo_queue(dir: &std::path::Path) -> Result<()> {
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).context("create .ralph dir")?;
    let queue_path = ralph_dir.join("queue.jsonc");
    let done_path = ralph_dir.join("done.jsonc");

    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Test task",
      "tags": ["rust"],
      "scope": ["crates/ralph"],
      "evidence": ["integration test fixture"],
      "plan": ["run preflight"],
      "request": "integration test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;

    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(&queue_path, queue).context("write queue.jsonc")?;
    std::fs::write(&done_path, done).context("write done.jsonc")?;
    Ok(())
}

/// Write a queue file with the given tasks.
pub fn write_queue(dir: &std::path::Path, tasks: &[Task]) -> Result<()> {
    let queue = QueueFile {
        version: 1,
        tasks: tasks.to_vec(),
    };
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let queue_path = ralph_dir.join("queue.jsonc");
    let json = serde_json::to_string_pretty(&queue)?;
    std::fs::write(&queue_path, json).with_context(|| "write queue.jsonc".to_string())?;
    Ok(())
}

/// Write a done file with the given tasks.
pub fn write_done(dir: &std::path::Path, tasks: &[Task]) -> Result<()> {
    let done = QueueFile {
        version: 1,
        tasks: tasks.to_vec(),
    };
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let done_path = ralph_dir.join("done.jsonc");
    let json = serde_json::to_string_pretty(&done)?;
    std::fs::write(&done_path, json).with_context(|| "write done.jsonc".to_string())?;
    Ok(())
}

/// Read the queue file from the given directory.
pub fn read_queue(dir: &std::path::Path) -> Result<QueueFile> {
    let queue_path = dir.join(".ralph/queue.jsonc");
    let raw = std::fs::read_to_string(&queue_path).context("read queue.jsonc")?;
    serde_json::from_str(&raw).context("parse queue.jsonc")
}

/// Read the done file from the given directory.
pub fn read_done(dir: &std::path::Path) -> Result<QueueFile> {
    let done_path = dir.join(".ralph/done.jsonc");
    let raw = std::fs::read_to_string(&done_path).context("read done.jsonc")?;
    serde_json::from_str(&raw).context("parse done.jsonc")
}

/// Snapshot of queue and done file contents for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDoneSnapshot {
    pub queue_json: String,
    pub done_json: String,
}

/// Snapshot queue and done files for later comparison.
pub fn snapshot_queue_done(dir: &std::path::Path) -> Result<QueueDoneSnapshot> {
    let queue_path = dir.join(".ralph/queue.jsonc");
    let done_path = dir.join(".ralph/done.jsonc");
    let queue_json = std::fs::read_to_string(&queue_path).context("read queue.jsonc")?;
    let done_json = std::fs::read_to_string(&done_path).context("read done.jsonc")?;
    Ok(QueueDoneSnapshot {
        queue_json,
        done_json,
    })
}
