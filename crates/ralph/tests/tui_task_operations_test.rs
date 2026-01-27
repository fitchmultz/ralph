//! Contract tests for TUI task operations.
//!
//! Focus: state mutations that operate on task list (status cycling, deletion, editing) and
//! timestamp policy when status changes.

mod test_support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ralph::contracts::{QueueFile, TaskStatus};
use ralph::timeutil;
use ralph::tui::{self, App, AppMode, TuiAction};
use test_support::{make_test_queue, make_test_task};

fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn canonical_rfc3339(ts: &str) -> String {
    let dt = timeutil::parse_rfc3339(ts).expect("valid RFC3339 timestamp");
    timeutil::format_rfc3339(dt).expect("format RFC3339 timestamp")
}

#[test]
fn test_cycle_status_cycles_correctly() {
    let mut app = App::new(make_test_queue());
    let task = app.queue.tasks[0].clone();
    assert_eq!(task.status, TaskStatus::Todo);

    let now1 = "2026-01-19T00:00:00Z";
    let now1_canon = canonical_rfc3339(now1);
    app.cycle_status(now1).unwrap();
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Doing);
    assert_eq!(app.queue.tasks[0].updated_at, Some(now1_canon));
    assert_eq!(app.queue.tasks[0].completed_at, None);

    let now2 = "2026-01-19T01:00:00Z";
    let now2_canon = canonical_rfc3339(now2);
    app.cycle_status(now2).unwrap();
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Done);
    assert_eq!(app.queue.tasks[0].completed_at, Some(now2_canon.clone()));

    let now3 = "2026-01-19T02:00:00Z";
    app.cycle_status(now3).unwrap();
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Rejected);
    assert_eq!(app.queue.tasks[0].completed_at, Some(now2_canon));

    app.cycle_status("2026-01-19T03:00:00Z").unwrap();
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Draft);
    assert_eq!(app.queue.tasks[0].completed_at, None);

    app.cycle_status("2026-01-19T04:00:00Z").unwrap();
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Todo);
    assert_eq!(app.queue.tasks[0].completed_at, None);
}

#[test]
fn test_cycle_status_sets_dirty_flag() {
    let mut app = App::new(make_test_queue());
    assert!(!app.dirty);

    app.cycle_status("2026-01-19T00:00:00Z").unwrap();
    assert!(app.dirty);
}

#[test]
fn test_cycle_status_fails_with_no_selection() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let mut app = App::new(queue);

    assert!(app.cycle_status("2026-01-19T00:00:00Z").is_err());
}

#[test]
fn test_delete_selected_task_removes_task() {
    let mut app = App::new(make_test_queue());
    app.selected = 1;

    let deleted = app.delete_selected_task().unwrap();
    assert_eq!(deleted.id, "RQ-0002");
    assert_eq!(app.queue.tasks.len(), 2);
    assert_eq!(app.queue.tasks[0].id, "RQ-0001");
    assert_eq!(app.queue.tasks[1].id, "RQ-0003");
}

#[test]
fn test_delete_selected_task_adjusts_selection_up() {
    let mut app = App::new(make_test_queue());
    app.selected = 2; // Last task

    app.delete_selected_task().unwrap();
    assert_eq!(app.selected, 1); // Should move to previous task
    assert_eq!(app.queue.tasks.len(), 2);
}

#[test]
fn test_delete_selected_task_when_only_one_task() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Only Task", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);
    app.selected = 0;

    app.delete_selected_task().unwrap();
    assert_eq!(app.queue.tasks.len(), 0);
    assert_eq!(app.selected, 0); // Should stay at 0
}

#[test]
fn test_delete_selected_task_sets_dirty_flag() {
    let mut app = App::new(make_test_queue());
    assert!(!app.dirty);

    app.delete_selected_task().unwrap();
    assert!(app.dirty);
}

#[test]
fn test_delete_selected_task_fails_with_no_selection() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let mut app = App::new(queue);

    assert!(app.delete_selected_task().is_err());
}

#[test]
fn test_editing_task_updates_title() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("New Title".to_string()),
    };

    let action =
        tui::handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T12:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].title, "New Title");
    assert_eq!(
        app.queue.tasks[0].updated_at,
        Some(canonical_rfc3339("2026-01-20T12:00:00Z"))
    );
    assert!(app.dirty);
}

#[test]
fn test_editing_task_rejects_empty_title() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("".to_string()),
    };

    let action =
        tui::handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T12:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: Some(_)
        }
    ));
    assert!(!app.dirty); // Should not set dirty on failure
}

#[test]
fn test_editing_task_fails_with_no_selection() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let mut app = App::new(queue);

    let action = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('e')),
        "2026-01-20T12:00:00Z",
    )
    .unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app
        .status_message
        .as_deref()
        .unwrap_or_default()
        .contains("No task"));
}

#[test]
fn test_timestamps_updated_on_status_change() {
    let mut app = App::new(make_test_queue());
    let task = &app.queue.tasks[0];

    assert_eq!(task.updated_at, Some("2026-01-19T00:00:00Z".to_string()));
    assert_eq!(task.completed_at, None);

    let status_now = "2026-01-20T12:34:56Z";
    let status_now_canon = canonical_rfc3339(status_now);
    app.cycle_status(status_now).unwrap();
    assert_eq!(app.queue.tasks[0].updated_at, Some(status_now_canon));

    let done_now = "2026-01-20T23:45:00Z";
    let done_now_canon = canonical_rfc3339(done_now);
    app.cycle_status(done_now).unwrap(); // Now Done
    assert_eq!(app.queue.tasks[0].updated_at, Some(done_now_canon.clone()));
    assert_eq!(
        app.queue.tasks[0].completed_at,
        Some(done_now_canon.clone())
    );

    app.cycle_status("2026-01-21T00:00:00Z").unwrap(); // Now Rejected
    assert_eq!(app.queue.tasks[0].completed_at, Some(done_now_canon));

    app.cycle_status("2026-01-21T01:00:00Z").unwrap(); // Back to Todo
    assert_eq!(app.queue.tasks[0].completed_at, None);
}
