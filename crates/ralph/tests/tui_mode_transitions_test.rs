//! Contract tests for TUI mode transitions.
//!
//! These tests focus on verifying `AppMode` changes caused by key events, independent of
//! rendering/terminal concerns.

mod test_support;

use crossterm::event::KeyCode;
use ralph::tui::{self, App, AppMode, TuiAction};
use test_support::make_test_queue;

#[test]
fn test_mode_transition_normal_to_editing() {
    let mut app = App::new(make_test_queue());
    assert_eq!(app.mode, AppMode::Normal);

    let _ = tui::handle_key_event(&mut app, KeyCode::Char('e'), "2026-01-19T00:00:00Z").unwrap();

    assert!(matches!(app.mode, AppMode::EditingTask { .. }));
}

#[test]
fn test_mode_transition_normal_to_delete() {
    let mut app = App::new(make_test_queue());
    assert_eq!(app.mode, AppMode::Normal);

    let _ = tui::handle_key_event(&mut app, KeyCode::Char('d'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(app.mode, AppMode::ConfirmDelete);
}

#[test]
fn test_mode_transition_normal_to_executing() {
    let mut app = App::new(make_test_queue());
    assert_eq!(app.mode, AppMode::Normal);

    let _ = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert!(matches!(app.mode, AppMode::Executing { .. }));
}

#[test]
fn test_mode_transition_editing_to_list_on_save() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("New Title".to_string()),
    };

    let _ = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: None
        }
    ));
}

#[test]
fn test_mode_transition_editing_to_list_on_cancel() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("New Title".to_string()),
    };

    let _ = tui::handle_key_event(&mut app, KeyCode::Esc, "2026-01-19T00:00:00Z").unwrap();

    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: None
        }
    ));
}

#[test]
fn test_mode_transition_delete_to_normal_on_confirm() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::ConfirmDelete;

    let _ = tui::handle_key_event(&mut app, KeyCode::Char('y'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_mode_transition_delete_to_normal_on_cancel() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::ConfirmDelete;

    let _ = tui::handle_key_event(&mut app, KeyCode::Char('n'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_mode_transition_executing_to_normal() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };

    let _ = tui::handle_key_event(&mut app, KeyCode::Esc, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_enter_key_in_executing_mode_does_not_quit() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    // Should continue, not quit
    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(app.mode, AppMode::Executing { .. }));
}
