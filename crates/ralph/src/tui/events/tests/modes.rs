//! Tests for mode transitions.
//!
//! Responsibilities:
//! - Test mode transitions (Normal -> Help, Normal -> Search, etc.).
//! - Validate help mode entry/exit.
//! - Test search/filter mode entry.
//! - Test create mode entry.
//!
//! Does NOT handle:
//! - Navigation within modes (see navigation.rs).
//! - Filter behavior (see filters.rs).
//! - Palette commands (see palette.rs).

use super::helpers::{input, key_event, make_queue, make_test_task};
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, TuiAction};
use crossterm::event::KeyCode;

#[test]
fn colon_enters_command_palette() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char(':')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::CommandPalette { .. } => {}
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn colon_enters_command_palette_with_empty_queue() {
    let mut app = App::new(make_queue(vec![]));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char(':')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::CommandPalette { .. } => {}
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn n_enters_create_mode_with_empty_queue() {
    let mut app = App::new(make_queue(vec![]));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('n')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTask(input("")));
}

#[test]
fn help_key_enters_help_mode() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('?')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn help_key_enters_help_mode_with_h() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('h')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn help_opens_from_search_and_returns_to_previous_mode() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::Searching(input("needle"));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('?')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
    assert!(matches!(
        app.help_previous_mode(),
        Some(AppMode::Searching(query)) if query.value() == "needle"
    ));

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Searching(input("needle")));
}

#[test]
fn help_key_does_not_interrupt_search_input() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::Searching(input(""));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('h')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Searching(input("h")));
}
