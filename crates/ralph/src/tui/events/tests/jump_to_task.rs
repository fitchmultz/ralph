//! Tests for jump-to-task functionality.
//!
//! Responsibilities:
//! - Test jump-to-task mode entry.
//! - Test task ID jumping.
//! - Test case-insensitive matching.
//! - Test not-found handling.
//!
//! Does NOT handle:
//! - Navigation behavior (see navigation.rs).
//! - Palette commands (see palette.rs).

use super::helpers::{input, key_event, make_queue, make_test_task};
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, PaletteCommand, TuiAction};
use crossterm::event::KeyCode;

#[test]
fn uppercase_g_enters_jump_to_task_mode() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('G')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(app.mode, AppMode::JumpingToTask(_)));
}

#[test]
fn palette_jump_to_task_command_enters_jump_mode() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::JumpToTask, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(matches!(app.mode, AppMode::JumpingToTask(_)));
}

#[test]
fn jump_to_task_mode_escape_cancels() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::JumpingToTask(input("RQ-0001"));

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn jump_to_task_by_id_success() {
    let queue = make_queue(vec![
        make_test_task("RQ-0001"),
        make_test_task("RQ-0002"),
        make_test_task("RQ-0003"),
    ]);
    let mut app = App::new(queue);
    app.selected = 0; // Start at RQ-0001

    let result = app.jump_to_task_by_id("RQ-0003");

    assert!(result);
    assert_eq!(app.selected, 2);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Jumped to task RQ-0003")
    );
}

#[test]
fn jump_to_task_by_id_case_insensitive() {
    let queue = make_queue(vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")]);
    let mut app = App::new(queue);

    // Lowercase input should match uppercase ID
    let result = app.jump_to_task_by_id("rq-0002");

    assert!(result);
    assert_eq!(app.selected, 1);
}

#[test]
fn jump_to_task_by_id_not_found() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let result = app.jump_to_task_by_id("RQ-9999");

    assert!(!result);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Task not found: RQ-9999")
    );
}

#[test]
fn jump_to_task_by_id_empty_input() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let result = app.jump_to_task_by_id("");

    assert!(!result);
    assert_eq!(app.status_message.as_deref(), Some("No task ID entered"));
}

#[test]
fn jump_to_task_by_id_with_active_filters() {
    let mut t1 = make_test_task("RQ-0001");
    t1.tags = vec!["alpha".to_string()];
    let t2 = make_test_task("RQ-0002"); // No tag 'alpha'
    let mut t3 = make_test_task("RQ-0003");
    t3.tags = vec!["alpha".to_string()];

    let queue = make_queue(vec![t1, t2, t3]);
    let mut app = App::new(queue);
    app.set_tag_filters(vec!["alpha".to_string()]);
    app.rebuild_filtered_view();

    // RQ-0002 is filtered out, but jump should find it and clear filters
    let result = app.jump_to_task_by_id("RQ-0002");

    assert!(result);
    assert_eq!(app.selected, 1); // RQ-0002 is now selected
    assert!(
        app.status_message
            .as_deref()
            .unwrap()
            .contains("filters cleared")
    );
}

#[test]
fn jump_to_task_mode_enter_executes_jump() {
    let queue = make_queue(vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")]);
    let mut app = App::new(queue);
    app.mode = AppMode::JumpingToTask(input("RQ-0002"));
    app.selected = 0;

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.selected, 1);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Jumped to task RQ-0002")
    );
}

#[test]
fn jump_to_task_mode_character_input() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::JumpingToTask(input(""));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('R')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::JumpingToTask(input("R")));
}

#[test]
fn jump_to_task_mode_backspace() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::JumpingToTask(input("RQ"));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Backspace),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::JumpingToTask(input("R")));
}

#[test]
fn jump_to_task_not_found_shows_error() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::JumpingToTask(input("RQ-9999"));

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Task not found: RQ-9999")
    );
}
