//! Test support helpers for queue search tests.
//!
//! Responsibilities:
//! - Provide shared task builder helpers for unit tests
//!
//! Not handled here:
//! - Production code (this is test-only)
//! - Complex test scenarios (handled in individual test modules)
//!
//! Invariants/assumptions:
//! - All tasks have minimal valid defaults
//! - Helpers are simple and composable

use crate::contracts::{Task, TaskStatus};
use std::collections::HashMap;

/// Create a basic task with default values for testing.
pub fn task(id: &str) -> Task {
    task_with(id, TaskStatus::Todo, vec!["code".to_string()])
}

/// Create a task with specified status and tags.
pub fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
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
    }
}

/// Create a task with specified scope.
pub fn task_with_scope(id: &str, scope: Vec<String>) -> Task {
    let mut t = task(id);
    t.scope = scope;
    t
}

/// Create a task with specified tags, scope, and status.
pub fn task_with_tags_scope_status(
    id: &str,
    tags: Vec<String>,
    scope: Vec<String>,
    status: TaskStatus,
) -> Task {
    let mut t = task(id);
    t.tags = tags;
    t.scope = scope;
    t.status = status;
    t
}
