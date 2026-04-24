//! Shared task fixtures for run-command tests.
//!
//! Purpose:
//! - Shared task fixtures for run-command tests.
//!
//! Responsibilities:
//! - Build representative queue tasks with stable defaults for run-test scenarios.
//! - Centralize task-shape setup so sibling suites avoid duplicating queue fixtures.
//!
//! Not handled here:
//! - Config or override builder helpers.
//! - Queue-lock or process-environment helpers.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Returned tasks match the queue schema expected by run-command tests.
//! - Callers may mutate the returned task copies without affecting other tests.

use crate::contracts::{Task, TaskStatus};

pub(crate) fn base_task() -> Task {
    Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Todo,
        title: "Test task".to_string(),
        description: None,
        priority: Default::default(),
        tags: vec!["rust".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
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
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    }
}

pub(crate) fn task_with_status(status: TaskStatus) -> Task {
    let mut task = base_task();
    task.status = status;
    task.request = Some("test request".to_string());
    task.created_at = Some("2026-01-18T00:00:00Z".to_string());
    task.updated_at = Some("2026-01-18T00:00:00Z".to_string());
    task
}

pub(crate) fn task_with_id_and_status(id: &str, status: TaskStatus) -> Task {
    let mut task = task_with_status(status);
    task.id = id.to_string();
    task
}
