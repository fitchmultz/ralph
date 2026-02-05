//! Unit tests for queue task operations (split by operation module).
//!
//! This module provides shared fixtures and shared imports, and delegates
//! to per-operation test modules in this directory.

pub(crate) use super::*;
pub(crate) use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::timeutil;
pub(crate) use std::collections::HashMap;

pub(crate) fn task(id: &str) -> Task {
    task_with(id, TaskStatus::Todo, vec!["code".to_string()])
}

pub(crate) fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
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

pub(crate) fn canonical_rfc3339(ts: &str) -> String {
    let dt = timeutil::parse_rfc3339(ts).expect("valid RFC3339 timestamp");
    timeutil::format_rfc3339(dt).expect("format RFC3339 timestamp")
}

mod archive;
mod batch;
mod edit;
mod fields;
mod mutation;
mod query;
mod status;
mod validate;
