//! Tests for task builder functionality.
//!
//! Responsibilities:
//! - Test task builder mode entry.
//! - Test task builder input handling.
//! - Test task creation via builder.
//! - Test builder cancellation.
//!
//! Does NOT handle:
//! - Simple task creation mode (see modes.rs).
//! - Palette commands (see palette.rs).

use super::helpers::{input, key_event, make_queue, make_test_task};
use crate::tui::app_palette_ops::PaletteOperations;
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, PaletteCommand, TuiAction};
use crossterm::event::KeyCode;

#[test]
fn uppercase_n_enters_task_builder_mode() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('N')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(app.mode, AppMode::CreatingTaskDescription(_)));
}

#[test]
fn task_builder_mode_handles_character_input() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CreatingTaskDescription(input(""));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('a')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTaskDescription(input("a")));
}

#[test]
fn task_builder_mode_handles_backspace() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CreatingTaskDescription(input("ab"));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Backspace),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTaskDescription(input("a")));
}

#[test]
fn task_builder_mode_escape_cancels() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CreatingTaskDescription(input("test description"));

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn task_builder_mode_empty_description_returns_to_normal() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CreatingTaskDescription(input(""));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Description cannot be empty")
    );
}

#[test]
fn task_builder_mode_whitespace_only_returns_to_normal() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CreatingTaskDescription(input("   "));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Description cannot be empty")
    );
}

#[test]
fn task_builder_mode_valid_description_builds_task() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CreatingTaskDescription(input("Add a new feature"));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(
        action,
        TuiAction::BuildTask("Add a new feature".to_string())
    );
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn palette_build_task_agent_command() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::BuildTaskAgent, "2026-01-20T00:00:00Z")
        .expect("execute command");

    // Now opens the advanced task builder flow instead of simple description input
    assert!(matches!(app.mode, AppMode::BuildingTaskOptions(_)));
}

#[test]
fn uppercase_n_rejected_when_runner_active() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.runner_active = true;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('N')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.status_message.as_deref(), Some("Runner already active"));
    assert_eq!(app.mode, AppMode::Normal);
}
