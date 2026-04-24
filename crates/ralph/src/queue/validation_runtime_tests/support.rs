//! Shared fixtures for queue validation runtime tests.
//!
//! Purpose:
//! - Shared fixtures for queue validation runtime tests.
//!
//! Responsibilities:
//! - Build queue task variants used across validation runtime tests.
//! - Keep timestamp/default field setup consistent for test readability.
//! - Avoid repeating task-construction boilerplate across behavior modules.
//!
//! Not handled here:
//! - Validation assertions or warning/error matching.
//! - Production task construction behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Default timestamps are valid RFC3339 UTC values.
//! - Helpers return structurally valid tasks unless a test mutates them.

use std::collections::HashMap;

use crate::contracts::{Task, TaskAgent, TaskStatus};

pub(super) fn task(id: &str) -> Task {
    task_with(id, TaskStatus::Todo, vec!["code".to_string()])
}

pub(super) fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        description: None,
        priority: Default::default(),
        tags,
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    }
}

pub(super) fn task_with_agent(id: &str, agent: TaskAgent) -> Task {
    let mut task = task(id);
    task.agent = Some(agent);
    task
}

pub(super) fn task_with_deps(id: &str, status: TaskStatus, deps: Vec<String>) -> Task {
    let mut task = task_with(id, status, Vec::new());
    task.scope.clear();
    task.evidence.clear();
    task.plan.clear();
    task.request = None;
    task.depends_on = deps;
    task
}

pub(super) fn task_with_relationships(
    id: &str,
    status: TaskStatus,
    blocks: Vec<String>,
    relates_to: Vec<String>,
    duplicates: Option<String>,
) -> Task {
    let mut task = task_with(id, status, Vec::new());
    task.scope.clear();
    task.evidence.clear();
    task.plan.clear();
    task.request = None;
    task.blocks = blocks;
    task.relates_to = relates_to;
    task.duplicates = duplicates;
    task
}

pub(super) fn task_with_parent(id: &str, parent_id: Option<&str>) -> Task {
    let mut task = task_with(id, TaskStatus::Todo, Vec::new());
    task.title = format!("Task {id}");
    task.scope.clear();
    task.evidence.clear();
    task.plan.clear();
    task.request = None;
    task.parent_id = parent_id.map(ToString::to_string);
    task
}
