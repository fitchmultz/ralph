//! Contract tests for TUI app initialization and task selection.
//!
//! These tests validate that `App::new` sets up consistent defaults and that selection helpers
//! behave correctly for both non-empty and empty queues.

mod test_support;

use ralph::contracts::QueueFile;
use ralph::tui::{App, AppMode};
use test_support::make_test_queue;

#[test]
fn test_app_new_initializes_state() {
    let queue = make_test_queue();
    let app = App::new(queue);

    assert_eq!(app.selected, 0);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.scroll, 0);
    assert_eq!(app.detail_width, 60);
    assert!(!app.dirty);
    assert!(app.logs.is_empty());
    assert_eq!(app.log_scroll, 0);
    assert!(app.autoscroll);
    assert_eq!(app.log_visible_lines, 20);
    assert_eq!(app.list_height, 20);
}

#[test]
fn test_app_new_with_empty_queue() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let app = App::new(queue);

    assert_eq!(app.selected, 0);
    assert_eq!(app.queue.tasks.len(), 0);
}

#[test]
fn test_selected_task_returns_current_selection() {
    let queue = make_test_queue();
    let app = App::new(queue);

    let task = app.selected_task().unwrap();
    assert_eq!(task.id, "RQ-0001");

    let mut app = App::new(make_test_queue());
    app.selected = 1;
    let task = app.selected_task().unwrap();
    assert_eq!(task.id, "RQ-0002");
}

#[test]
fn test_selected_task_returns_none_when_empty() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let app = App::new(queue);

    assert!(app.selected_task().is_none());
}
