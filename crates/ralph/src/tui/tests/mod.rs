//! Test modules for the TUI application state.
//!
//! Responsibilities:
//! - Provide shared test helpers and imports for all TUI test modules.
//! - Re-export test modules for app state, filters, navigation, palette, and phase tracking.
//!
//! Not handled here:
//! - Terminal rendering, input polling, or cross-process execution.
//! - Persistence/locking integration beyond in-memory helpers.
//!
//! Invariants/assumptions:
//! - Tests operate on in-memory queues with deterministic timestamps.
//! - File IO uses temporary directories for isolation.

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::timeutil;
use anyhow::Result;

// Re-export test modules
mod app_state;
mod filters;
mod navigation;
mod palette;
mod phase_tracking;

/// Creates a test task with default values.
pub fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        title: title.to_string(),
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
        completed_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
    }
}

/// Normalizes RFC3339 timestamps for comparison.
pub fn canonical_rfc3339(ts: &str) -> String {
    let dt = timeutil::parse_rfc3339(ts).expect("valid RFC3339 timestamp");
    timeutil::format_rfc3339(dt).expect("format RFC3339 timestamp")
}

/// Creates a test task with specific tags.
pub fn make_test_task_with_tags(id: &str, title: &str, tags: Vec<&str>) -> Task {
    let mut task = make_test_task(id, title, TaskStatus::Todo);
    task.tags = tags.into_iter().map(|tag| tag.to_string()).collect();
    task
}
