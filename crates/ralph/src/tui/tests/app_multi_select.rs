//! Tests for multi-select operations in the TUI.

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::tui::app::App;
use crate::tui::app_filters::FilterManagementOperations;
use crate::tui::app_multi_select::MultiSelectOperations;
use std::collections::HashMap;

fn create_test_task(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        title: format!("Task {}", id),
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
        parent_id: None,
    }
}

fn create_test_app_with_tasks() -> App {
    let queue = QueueFile {
        version: 1,
        tasks: vec![
            create_test_task("RQ-0001", TaskStatus::Todo),
            create_test_task("RQ-0002", TaskStatus::Doing),
            create_test_task("RQ-0003", TaskStatus::Done),
            create_test_task("RQ-0004", TaskStatus::Todo),
            create_test_task("RQ-0005", TaskStatus::Rejected),
        ],
    };
    App::new(queue)
}

#[test]
fn test_multi_select_mode_toggle() {
    let mut app = create_test_app_with_tasks();

    // Initially off
    assert!(!app.multi_select_mode);
    assert!(app.selected_indices.is_empty());

    // Toggle on
    app.toggle_multi_select_mode();
    assert!(app.multi_select_mode);

    // Toggle off clears selection
    app.selected_indices.insert(0);
    app.selected_indices.insert(2);
    app.toggle_multi_select_mode();
    assert!(!app.multi_select_mode);
    assert!(app.selected_indices.is_empty());
}

#[test]
fn test_toggle_current_selection() {
    let mut app = create_test_app_with_tasks();

    // Enable multi-select mode
    app.toggle_multi_select_mode();
    app.selected = 1; // Select second task

    // Toggle selection on
    app.toggle_current_selection();
    assert!(app.is_selected(1));
    assert_eq!(app.selection_count(), 1);

    // Toggle selection off
    app.toggle_current_selection();
    assert!(!app.is_selected(1));
    assert_eq!(app.selection_count(), 0);
}

#[test]
fn test_toggle_current_selection_no_op_when_not_in_multi_select() {
    let mut app = create_test_app_with_tasks();

    // Not in multi-select mode
    assert!(!app.multi_select_mode);
    app.selected = 1;

    // Should be no-op
    app.toggle_current_selection();
    assert!(!app.is_selected(1));
    assert_eq!(app.selection_count(), 0);
}

#[test]
fn test_clear_selection() {
    let mut app = create_test_app_with_tasks();

    app.toggle_multi_select_mode();
    app.selected_indices.insert(0);
    app.selected_indices.insert(2);
    app.selected_indices.insert(4);

    assert_eq!(app.selection_count(), 3);
    assert!(app.multi_select_mode);

    app.clear_selection();

    assert!(app.selected_indices.is_empty());
    assert!(!app.multi_select_mode);
}

#[test]
fn test_batch_delete_by_filtered_indices() {
    let mut app = create_test_app_with_tasks();
    let initial_count = app.queue.tasks.len();

    // Select tasks at filtered positions 0 and 2
    let deleted = app.batch_delete_by_filtered_indices(&[0, 2]).unwrap();

    assert_eq!(deleted, 2);
    assert_eq!(app.queue.tasks.len(), initial_count - 2);
    assert!(app.dirty);
}

#[test]
fn test_batch_delete_empty_selection() {
    let mut app = create_test_app_with_tasks();
    let initial_count = app.queue.tasks.len();

    let deleted = app.batch_delete_by_filtered_indices(&[]).unwrap();

    assert_eq!(deleted, 0);
    assert_eq!(app.queue.tasks.len(), initial_count);
}

#[test]
fn test_batch_archive_by_filtered_indices() {
    let mut app = create_test_app_with_tasks();
    let initial_queue_count = app.queue.tasks.len();
    let initial_done_count = app.done.tasks.len();

    // Select tasks at filtered positions 1 and 3
    let archived = app
        .batch_archive_by_filtered_indices(&[1, 3], "2024-01-01T00:00:00Z")
        .unwrap();

    assert_eq!(archived, 2);
    assert_eq!(app.queue.tasks.len(), initial_queue_count - 2);
    assert_eq!(app.done.tasks.len(), initial_done_count + 2);
    assert!(app.dirty);
    assert!(app.dirty_done);
    // Selection should be cleared after archive
    assert!(app.selected_indices.is_empty());
    assert!(!app.multi_select_mode);
}

#[test]
fn test_batch_archive_empty_selection() {
    let mut app = create_test_app_with_tasks();
    let initial_queue_count = app.queue.tasks.len();

    let archived = app
        .batch_archive_by_filtered_indices(&[], "2024-01-01T00:00:00Z")
        .unwrap();

    assert_eq!(archived, 0);
    assert_eq!(app.queue.tasks.len(), initial_queue_count);
}

#[test]
fn test_selection_persists_across_filter_changes() {
    let mut app = create_test_app_with_tasks();

    app.toggle_multi_select_mode();
    app.selected = 1;
    app.toggle_current_selection();
    app.selected = 3;
    app.toggle_current_selection();

    assert_eq!(app.selection_count(), 2);
    assert!(app.is_selected(1));
    assert!(app.is_selected(3));

    // Change filters (this rebuilds filtered view)
    app.clear_filters();

    // Selection indices are preserved (they refer to filtered positions)
    assert_eq!(app.selection_count(), 2);
}
