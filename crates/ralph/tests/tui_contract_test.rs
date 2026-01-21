//! Contract tests for TUI state management and event handling.
//!
//! These tests verify the core contracts of the TUI without requiring
//! a full terminal or rendering backend.

use crossterm::event::KeyCode;
use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use ralph::tui::{self, App, AppMode, TuiAction};

/// Helper to create a test task.
fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
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
        depends_on: vec![],
        custom_fields: std::collections::HashMap::new(),
    }
}

/// Helper to create a test queue with multiple tasks.
fn make_test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![
            make_test_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_test_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_test_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}

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

#[test]
fn test_move_up_decrements_selection() {
    let mut app = App::new(make_test_queue());
    app.selected = 2;

    app.move_up();
    assert_eq!(app.selected, 1);

    app.move_up();
    assert_eq!(app.selected, 0);
}

#[test]
fn test_move_up_does_not_go_below_zero() {
    let mut app = App::new(make_test_queue());
    app.selected = 0;

    app.move_up();
    assert_eq!(app.selected, 0);
}

#[test]
fn test_move_up_adjusts_scroll() {
    let mut app = App::new(make_test_queue());
    app.selected = 5;
    app.scroll = 5;

    app.move_up();
    assert_eq!(app.selected, 4);
    assert_eq!(app.scroll, 4); // scroll should follow selection
}

#[test]
fn test_move_down_increments_selection() {
    let mut app = App::new(make_test_queue());
    app.selected = 0;

    app.move_down(10);
    assert_eq!(app.selected, 1);

    app.move_down(10);
    assert_eq!(app.selected, 2);
}

#[test]
fn test_move_down_stays_within_bounds() {
    let mut app = App::new(make_test_queue());
    app.selected = 2; // Last index

    app.move_down(10);
    assert_eq!(app.selected, 2); // Should not exceed bounds
}

#[test]
fn test_move_down_adjusts_scroll() {
    // Create a larger queue to test scrolling
    let tasks: Vec<Task> = (0..15)
        .map(|i| {
            make_test_task(
                &format!("RQ-{:04}", i),
                &format!("Task {}", i),
                TaskStatus::Todo,
            )
        })
        .collect();
    let queue = QueueFile { version: 1, tasks };
    let mut app = App::new(queue);
    app.selected = 0;
    app.scroll = 0;

    // Move past visible area (list_height = 10)
    for _ in 0..11 {
        app.move_down(10);
    }
    // Scroll should adjust to keep selection visible
    assert!(app.scroll > 0);
}

#[test]
fn test_cycle_status_cycles_correctly() {
    let mut app = App::new(make_test_queue());
    let task = app.queue.tasks[0].clone();
    assert_eq!(task.status, TaskStatus::Todo);

    app.cycle_status("2026-01-19T00:00:00Z").unwrap();
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Doing);
    assert_eq!(
        app.queue.tasks[0].updated_at,
        Some("2026-01-19T00:00:00Z".to_string())
    );
    assert_eq!(app.queue.tasks[0].completed_at, None);

    app.cycle_status("2026-01-19T01:00:00Z").unwrap();
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Done);
    assert_eq!(
        app.queue.tasks[0].completed_at,
        Some("2026-01-19T01:00:00Z".to_string())
    );

    app.cycle_status("2026-01-19T02:00:00Z").unwrap();
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Rejected);
    assert_eq!(
        app.queue.tasks[0].completed_at,
        Some("2026-01-19T02:00:00Z".to_string())
    );

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
fn test_update_title_changes_title() {
    let mut app = App::new(make_test_queue());

    app.update_title("New Title".to_string()).unwrap();
    assert_eq!(app.queue.tasks[0].title, "New Title");
    assert!(app.dirty);
}

#[test]
fn test_update_title_rejects_empty_title() {
    let mut app = App::new(make_test_queue());

    assert!(app.update_title("".to_string()).is_err());
    assert!(app.update_title("   ".to_string()).is_err());
    assert!(!app.dirty); // Should not set dirty on failure
}

#[test]
fn test_update_title_fails_with_no_selection() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let mut app = App::new(queue);

    assert!(app.update_title("New Title".to_string()).is_err());
}

// Event handling tests

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
    match &app.mode {
        AppMode::EditingTitle(title) => assert_eq!(title, "First Task"),
        _ => panic!("Expected EditingTitle mode"),
    }
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
    app.mode = AppMode::EditingTitle("Test".to_string());

    let action =
        tui::handle_key_event(&mut app, KeyCode::Char('X'), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::EditingTitle(title) => assert_eq!(title, "TestX"),
        _ => panic!("Expected EditingTitle mode"),
    }
}

#[test]
fn test_handle_key_event_in_editing_mode_backspace() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTitle("Test".to_string());

    let action =
        tui::handle_key_event(&mut app, KeyCode::Backspace, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::EditingTitle(title) => assert_eq!(title, "Tes"),
        _ => panic!("Expected EditingTitle mode"),
    }
}

#[test]
fn test_handle_key_event_in_editing_mode_enter_saves() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTitle("New Title".to_string());

    let action = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.queue.tasks[0].title, "New Title");
    assert!(app.dirty);
}

#[test]
fn test_handle_key_event_in_editing_mode_esc_cancels() {
    let mut app = App::new(make_test_queue());
    let original_title = app.queue.tasks[0].title.clone();
    app.mode = AppMode::EditingTitle("Modified Title".to_string());

    let action = tui::handle_key_event(&mut app, KeyCode::Esc, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.queue.tasks[0].title, original_title); // Title unchanged
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

// Mode transition tests

#[test]
fn test_mode_transition_normal_to_editing() {
    let mut app = App::new(make_test_queue());
    assert_eq!(app.mode, AppMode::Normal);

    let _ = tui::handle_key_event(&mut app, KeyCode::Char('e'), "2026-01-19T00:00:00Z").unwrap();

    assert!(matches!(app.mode, AppMode::EditingTitle(_)));
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
fn test_mode_transition_editing_to_normal_on_save() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTitle("New Title".to_string());

    let _ = tui::handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_mode_transition_editing_to_normal_on_cancel() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTitle("New Title".to_string());

    let _ = tui::handle_key_event(&mut app, KeyCode::Esc, "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(app.mode, AppMode::Normal);
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

#[test]
fn test_timestamps_updated_on_status_change() {
    let mut app = App::new(make_test_queue());
    let task = &app.queue.tasks[0];

    assert_eq!(task.updated_at, Some("2026-01-19T00:00:00Z".to_string()));
    assert_eq!(task.completed_at, None);

    app.cycle_status("2026-01-20T12:34:56Z").unwrap();
    assert_eq!(
        app.queue.tasks[0].updated_at,
        Some("2026-01-20T12:34:56Z".to_string())
    );

    app.cycle_status("2026-01-20T23:45:00Z").unwrap(); // Now Done
    assert_eq!(
        app.queue.tasks[0].updated_at,
        Some("2026-01-20T23:45:00Z".to_string())
    );
    assert_eq!(
        app.queue.tasks[0].completed_at,
        Some("2026-01-20T23:45:00Z".to_string())
    );

    app.cycle_status("2026-01-21T00:00:00Z").unwrap(); // Now Rejected
    assert_eq!(
        app.queue.tasks[0].completed_at,
        Some("2026-01-21T00:00:00Z".to_string())
    );

    app.cycle_status("2026-01-21T01:00:00Z").unwrap(); // Back to Todo
    assert_eq!(app.queue.tasks[0].completed_at, None);
}
