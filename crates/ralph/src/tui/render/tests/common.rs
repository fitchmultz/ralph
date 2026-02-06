//! Shared test utilities for TUI render tests.
//!
//! Responsibilities:
//! - Provide helper functions for converting buffers to strings for assertions.
//! - Provide factory functions for creating test queue data.
//!
//! Not handled here:
//! - Actual test assertions (see individual test modules).
//! - Test setup for specific components.

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::tui::App;
use crate::tui::render::footer;
use ratatui::buffer::Buffer;
use ratatui::text::Span;
use std::collections::HashMap;

/// Convert a slice of spans to a string for assertions.
pub fn spans_to_string(spans: &[Span<'static>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

/// Get footer text for testing.
pub fn footer_text(app: &App, width: usize) -> String {
    spans_to_string(&footer::help_footer_spans(app, width))
}

/// Convert a buffer to a string representation.
pub fn buffer_to_string(buffer: &Buffer) -> String {
    let mut lines = Vec::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            let cell = buffer.cell((x, y)).expect("cell in buffer");
            line.push_str(cell.symbol());
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Extract a specific line from a buffer.
pub fn buffer_line(buffer: &Buffer, x: u16, y: u16, width: u16) -> String {
    let mut line = String::new();
    for offset in 0..width {
        let cell = buffer.cell((x + offset, y)).expect("cell in buffer");
        line.push_str(cell.symbol());
    }
    line.trim_end().to_string()
}

/// Create a queue with long evidence and plan for scroll testing.
pub fn make_long_details_queue() -> QueueFile {
    let evidence: Vec<String> = (0..20).map(|i| format!("Evidence line {i}")).collect();
    let plan: Vec<String> = (0..10).map(|i| format!("Plan step {i}")).collect();
    QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0001".to_string(),
            title: "Long Task".to_string(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence,
            plan,
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }],
    }
}

/// Create a queue with many tags for wrap testing.
pub fn make_long_tags_queue() -> QueueFile {
    let tags: Vec<String> = (0..40).map(|i| format!("very-long-tag-{i:02}")).collect();
    QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0002".to_string(),
            title: "Tagged Task".to_string(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Low,
            tags,
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }],
    }
}

/// Create a queue with 3 sample tasks for list testing.
pub fn make_task_list_queue() -> QueueFile {
    let make_task = |id: &str, title: &str, status: TaskStatus| Task {
        id: id.to_string(),
        title: title.to_string(),
        description: None,
        status,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    };

    QueueFile {
        version: 1,
        tasks: vec![
            make_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}
