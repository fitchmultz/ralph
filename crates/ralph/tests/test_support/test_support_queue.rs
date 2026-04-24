//! Queue-centric fixture and assertion helpers for integration tests.
//!
//! Purpose:
//! - Queue-centric fixture and assertion helpers for integration tests.
//!
//! Responsibilities:
//! - Build realistic queue/task fixtures without inline JSON blobs in scenario suites.
//! - Write queue files and read back stable task ordering assertions.
//! - Keep queue-list and queue-sort tests focused on behavior instead of serialization boilerplate.
//!
//! Non-scope:
//! - Repo initialization or command execution.
//! - Queue validation semantics outside fixture construction.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Fixtures target the v1 queue contract.
//! - Timestamp fields must already be RFC3339 strings when provided.
//! - Task ID extraction assumes `queue list` tab-separated output.

use anyhow::{Context, Result};
use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TaskBuilder {
    task: Task,
}

impl TaskBuilder {
    pub fn new(id: &str, title: &str, status: TaskStatus, priority: TaskPriority) -> Self {
        Self {
            task: Task {
                id: id.to_string(),
                status,
                title: title.to_string(),
                description: None,
                priority,
                tags: vec!["test".to_string()],
                scope: vec![],
                evidence: vec![],
                plan: vec![],
                notes: vec![],
                request: None,
                agent: None,
                created_at: None,
                updated_at: None,
                completed_at: None,
                started_at: None,
                scheduled_start: None,
                depends_on: vec![],
                blocks: vec![],
                relates_to: vec![],
                duplicates: None,
                custom_fields: HashMap::new(),
                estimated_minutes: None,
                actual_minutes: None,
                parent_id: None,
            },
        }
    }

    pub fn tags(mut self, tags: &[&str]) -> Self {
        self.task.tags = tags.iter().map(|tag| (*tag).to_string()).collect();
        self
    }

    pub fn scope(mut self, scope: &[&str]) -> Self {
        self.task.scope = scope.iter().map(|entry| (*entry).to_string()).collect();
        self
    }

    pub fn evidence(mut self, evidence: &[&str]) -> Self {
        self.task.evidence = evidence.iter().map(|entry| (*entry).to_string()).collect();
        self
    }

    pub fn plan(mut self, plan: &[&str]) -> Self {
        self.task.plan = plan.iter().map(|entry| (*entry).to_string()).collect();
        self
    }

    pub fn request(mut self, request: &str) -> Self {
        self.task.request = Some(request.to_string());
        self
    }

    pub fn created_at(mut self, timestamp: &str) -> Self {
        self.task.created_at = Some(timestamp.to_string());
        self
    }

    pub fn updated_at(mut self, timestamp: &str) -> Self {
        self.task.updated_at = Some(timestamp.to_string());
        self
    }

    pub fn started_at(mut self, timestamp: Option<&str>) -> Self {
        self.task.started_at = timestamp.map(str::to_string);
        self
    }

    pub fn scheduled_start(mut self, timestamp: Option<&str>) -> Self {
        self.task.scheduled_start = timestamp.map(str::to_string);
        self
    }

    pub fn completed_at(mut self, timestamp: Option<&str>) -> Self {
        self.task.completed_at = timestamp.map(str::to_string);
        self
    }

    pub fn build(self) -> Task {
        self.task
    }
}

pub fn write_queue_file(dir: &Path, tasks: impl IntoIterator<Item = Task>) -> Result<()> {
    let queue = QueueFile {
        version: 1,
        tasks: tasks.into_iter().collect(),
    };
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).context("create .ralph dir")?;
    let queue_path = ralph_dir.join("queue.jsonc");
    std::fs::write(
        &queue_path,
        serde_json::to_string_pretty(&queue).context("serialize queue fixture")?,
    )
    .with_context(|| format!("write {}", queue_path.display()))?;
    Ok(())
}

pub fn read_queue_task_ids(dir: &Path) -> Result<Vec<String>> {
    let queue_path = dir.join(".ralph/queue.jsonc");
    let raw = std::fs::read_to_string(&queue_path)
        .with_context(|| format!("read {}", queue_path.display()))?;
    let queue: QueueFile =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", queue_path.display()))?;
    Ok(queue.tasks.into_iter().map(|task| task.id).collect())
}

pub fn tab_separated_ids(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.split('\t').next())
        .map(str::to_string)
        .collect()
}
