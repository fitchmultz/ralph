//! Contract tests for TUI key event handling.
//!
//! These tests exercise `tui::handle_key_event` across multiple `AppMode` states, ensuring key
//! bindings perform the correct state transitions and side effects.

mod test_support;

use crossterm::event::KeyCode;
use ralph::contracts::{TaskPriority, TaskStatus};
use ralph::runutil::RevertDecision;
use ralph::tui::{self, App, AppMode, TuiAction};
use std::sync::mpsc;
use test_support::make_test_queue;

#[test]
fn test_handle_key_event_quit_with_q() {
    let mut app = App::new(make_test_queue());
    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('q'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Quit);
}

#[test]
fn test_handle_key_event_quit_with_esc() {
    let mut app = App::new(make_test_queue());
    let action = tui::handle_key_event(&mut app, KeyCode::Esc, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Quit);
}

#[test]
fn test_handle_key_event_navigation_up() {
    let mut app = App::new(make_test_queue());
    app.selected = 1;

    let action = tui::handle_key_event(&mut app, KeyCode::Up, "2026-01-19T00:00:00Z").unwrap();
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 0);

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('k'), "2026-01-19T00:00:00Z").unwrap();
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 0); // Should not go below 0
}

#[test]
fn test_handle_key_event_navigation_down() {
    let mut app = App::new(make_test_queue());

    let action = tui::handle_key_event(&mut app, KeyCode::Down, "2026-01-19T00:00:00Z").unwrap();
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 1);

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('j'), "2026-01-19T00:00:00Z").unwrap();
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 2);
}

#[test]
fn test_handle_key_event_enter_runs_task() {
    let mut app = App::new(make_test_queue());

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    match action {
        TuiAction::RunTask(task_id) => assert_eq!(task_id, "RQ-0001"),
        _ => panic!("Expected RunTask action"),
    }
    assert_eq!(
        app.mode,
        AppMode::Executing {
            task_id: "RQ-0001".to_string()
        }
    );
    assert!(app.logs.is_empty());
    assert_eq!(app.log_scroll, 0);
}

#[test]
fn test_handle_key_event_d_enters_delete_mode() {
    let mut app = App::new(make_test_queue());

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('d'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::ConfirmDelete);
}

#[test]
fn test_handle_key_event_e_enters_edit_mode() {
    let mut app = App::new(make_test_queue());

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('e'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: None
        }
    ));
}

#[test]
fn test_handle_key_event_s_cycles_status() {
    let mut app = App::new(make_test_queue());
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Todo);

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('s'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Doing);
    assert!(app.dirty);
}

#[test]
fn test_handle_key_event_in_editing_mode_char() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("Test".to_string()),
    };

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('X'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::EditingTask {
            selected,
            editing_value,
        } => {
            assert_eq!(*selected, 0);
            assert_eq!(editing_value.as_deref(), Some("TestX"));
        }
        _ => panic!("Expected EditingTask mode"),
    }
}

#[test]
fn test_handle_key_event_in_editing_mode_backspace() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("Test".to_string()),
    };

    let action =
        tui::handle_key_event(&mut app, KeyCode::Backspace, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::EditingTask {
            selected,
            editing_value,
        } => {
            assert_eq!(*selected, 0);
            assert_eq!(editing_value.as_deref(), Some("Tes"));
        }
        _ => panic!("Expected EditingTask mode"),
    }
}

#[test]
fn test_handle_key_event_in_editing_mode_enter_saves() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("New Title".to_string()),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: None
        }
    ));
    assert_eq!(app.queue.tasks[0].title, "New Title");
    assert!(app.dirty);
}

#[test]
fn test_handle_key_event_in_editing_mode_esc_cancels() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("Modified Title".to_string()),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Esc, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: None
        }
    ));
}

#[test]
fn test_handle_key_event_editing_task_cycles_status() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 1,
        editing_value: None,
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T12:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Doing);
    assert!(app.dirty);
    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 1,
            editing_value: None
        }
    ));
}

#[test]
fn test_handle_key_event_editing_task_cycles_priority() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 2,
        editing_value: None,
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T12:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].priority, TaskPriority::High);
    assert!(app.dirty);
    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 2,
            editing_value: None
        }
    ));
}

#[test]
fn test_handle_key_event_editing_task_clears_list() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 3,
        editing_value: None,
    };

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('x'), "2026-01-20T12:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert!(app.queue.tasks[0].tags.is_empty());
    assert!(app.dirty);
}

#[test]
fn test_handle_key_event_editing_task_clears_custom_fields() {
    let mut app = App::new(make_test_queue());
    app.queue.tasks[0]
        .custom_fields
        .insert("owner".to_string(), "tui".to_string());
    app.mode = AppMode::EditingTask {
        selected: 10,
        editing_value: None,
    };

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('x'), "2026-01-20T12:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert!(app.queue.tasks[0].custom_fields.is_empty());
    assert!(app.dirty);
}

#[test]
fn test_handle_key_event_editing_task_validation_error_keeps_editing() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("".to_string()),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T12:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: Some(_)
        }
    ));
    assert!(app
        .status_message
        .as_deref()
        .unwrap_or_default()
        .contains("Error"));
    assert_eq!(app.queue.tasks[0].title, "First Task");
}

#[test]
fn test_handle_key_event_in_confirm_delete_y_deletes() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::ConfirmDelete;

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('y'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.queue.tasks.len(), 2); // Task deleted
}

#[test]
fn test_handle_key_event_in_confirm_delete_n_cancels() {
    let mut app = App::new(make_test_queue());
    let original_len = app.queue.tasks.len();
    app.mode = AppMode::ConfirmDelete;

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('n'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.queue.tasks.len(), original_len); // Task not deleted
}

#[test]
fn test_handle_key_event_in_confirm_delete_esc_cancels() {
    let mut app = App::new(make_test_queue());
    let original_len = app.queue.tasks.len();
    app.mode = AppMode::ConfirmDelete;

    let action = tui::handle_key_event(&mut app, KeyCode::Esc, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.queue.tasks.len(), original_len); // Task not deleted
}

#[test]
fn test_handle_key_event_in_confirm_revert_reverts() {
    let mut app = App::new(make_test_queue());
    let (tx, rx) = mpsc::channel();
    app.mode = AppMode::ConfirmRevert {
        label: "Phase 2 CI failure".to_string(),
        allow_proceed: false,
        selected: 1,
        input: String::new(),
        reply_sender: tx,
        previous_mode: Box::new(AppMode::Normal),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(rx.try_recv().unwrap(), RevertDecision::Revert);
}

#[test]
fn test_handle_key_event_in_confirm_revert_keeps() {
    let mut app = App::new(make_test_queue());
    let (tx, rx) = mpsc::channel();
    app.mode = AppMode::ConfirmRevert {
        label: "Phase 2 CI failure".to_string(),
        allow_proceed: false,
        selected: 0,
        input: String::new(),
        reply_sender: tx,
        previous_mode: Box::new(AppMode::Normal),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(rx.try_recv().unwrap(), RevertDecision::Keep);
}

#[test]
fn test_handle_key_event_in_confirm_revert_continues_with_message() {
    let mut app = App::new(make_test_queue());
    let (tx, rx) = mpsc::channel();
    app.mode = AppMode::ConfirmRevert {
        label: "Phase 2 CI failure".to_string(),
        allow_proceed: false,
        selected: 2,
        input: "Please continue".to_string(),
        reply_sender: tx,
        previous_mode: Box::new(AppMode::Normal),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(
        rx.try_recv().unwrap(),
        RevertDecision::Continue {
            message: "Please continue".to_string()
        }
    );
}

#[test]
fn test_handle_key_event_in_confirm_revert_requires_message() {
    let mut app = App::new(make_test_queue());
    let (tx, rx) = mpsc::channel();
    app.mode = AppMode::ConfirmRevert {
        label: "Phase 2 CI failure".to_string(),
        allow_proceed: false,
        selected: 2,
        input: String::new(),
        reply_sender: tx,
        previous_mode: Box::new(AppMode::Normal),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(app.mode, AppMode::ConfirmRevert { .. }));
    assert!(rx.try_recv().is_err());
}

#[test]
fn test_handle_key_event_in_confirm_revert_enter_defaults_keep() {
    let mut app = App::new(make_test_queue());
    let (tx, rx) = mpsc::channel();
    app.mode = AppMode::ConfirmRevert {
        label: "Phase 2 CI failure".to_string(),
        allow_proceed: false,
        selected: 0,
        input: String::new(),
        reply_sender: tx,
        previous_mode: Box::new(AppMode::Normal),
    };

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(rx.try_recv().unwrap(), RevertDecision::Keep);
}

#[test]
fn test_handle_key_event_in_executing_mode_esc_returns() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.logs.push("Log line 1".to_string());
    app.logs.push("Log line 2".to_string());

    let action = tui::handle_key_event(&mut app, KeyCode::Esc, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    // Logs should be preserved
    assert_eq!(app.logs.len(), 2);
}

#[test]
fn test_handle_key_event_unknown_key_continues() {
    let mut app = App::new(make_test_queue());
    let original_selected = app.selected;

    // Unknown key in normal mode
    let action = tui::handle_key_event(&mut app, KeyCode::Tab, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, original_selected);
}
