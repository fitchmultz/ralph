//! Test helpers for TUI event handling tests.
//!
//! Responsibilities:
//! - Provide shared test utilities for creating synthetic input events.
//! - Create test tasks with deterministic data.
//! - Provide assertion helpers for common test patterns.
//!
//! Does NOT handle:
//! - Test logic or assertions specific to functionality areas.
//! - State management or test setup beyond basic helpers.

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::tui::TextInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

/// Create a basic key event with no modifiers.
pub fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Create a Ctrl+key event.
pub fn ctrl_key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

/// Create a mouse event with the given kind and position.
pub fn mouse_event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Create a text input with the given value.
pub fn input(value: &str) -> TextInput {
    TextInput::new(value)
}

/// Create a text input with the given value and cursor position.
pub fn input_with_cursor(value: &str, cursor: usize) -> TextInput {
    TextInput::from_parts(value, cursor)
}

/// Create a test task with the given ID and default values.
pub fn make_test_task(id: &str) -> Task {
    Task {
        id: id.to_string(),
        title: "Test task".to_string(),
        status: TaskStatus::Todo,
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
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
    }
}

/// Create a QueueFile with the given tasks.
pub fn make_queue(tasks: Vec<Task>) -> QueueFile {
    QueueFile { version: 1, tasks }
}
