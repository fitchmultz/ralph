//! Tests for command palette functionality.
//!
//! Responsibilities:
//! - Test command palette entry and exit.
//! - Test palette command execution.
//! - Test palette navigation (up/down).
//! - Test palette query input.
//! - Test toggle commands via palette.
//!
//! Does NOT handle:
//! - Mode transitions to/from palette (see modes.rs).
//! - Specific command implementations (see task_ops.rs, config.rs, etc.).

use super::helpers::{ctrl_key_event, input, key_event, make_queue, make_test_task};
use crate::tui::app_palette_ops::PaletteOperations;
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, PaletteCommand, TuiAction};
use crossterm::event::KeyCode;

#[test]
fn command_palette_runs_selected_command() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::CommandPalette {
        query: input("run selected"),
        selected: 0,
    };

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::RunTask("RQ-0001".to_string()));
    assert!(app.runner_active);
}

#[test]
fn command_palette_with_no_matches_sets_status_message() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::CommandPalette {
        query: input("nope"),
        selected: 0,
    };

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.status_message.as_deref(), Some("No matching command"));
}

#[test]
fn command_palette_typing_jk_appends_query_and_resets_selection() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CommandPalette {
        query: input("ru"),
        selected: 4,
    };

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('j')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "ruj");
            assert_eq!(*selected, 0);
        }
        other => panic!("expected command palette, got {:?}", other),
    }

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('k')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "rujk");
            assert_eq!(*selected, 0);
        }
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn command_palette_up_down_navigation_preserves_query() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CommandPalette {
        query: input("run"),
        selected: 1,
    };

    let action = handle_key_event(&mut app, key_event(KeyCode::Up), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "run");
            assert_eq!(*selected, 0);
        }
        other => panic!("expected command palette, got {:?}", other),
    }

    let action = handle_key_event(&mut app, key_event(KeyCode::Down), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "run");
            assert_eq!(*selected, 1);
        }
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn command_palette_ctrl_char_is_ignored_for_text_entry() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::CommandPalette {
        query: input("run"),
        selected: 2,
    };

    let action = handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('j')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "run");
            assert_eq!(*selected, 2);
        }
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn palette_toggle_case_sensitive_command() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::ToggleCaseSensitive, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(app.filters.search_options.case_sensitive);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Case-sensitive search enabled")
    );
}

#[test]
fn palette_toggle_regex_command() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::ToggleRegex, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(app.filters.search_options.use_regex);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Regex search enabled (fuzzy disabled)")
    );
}

#[test]
fn palette_filter_scopes_command() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::FilterScopes, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(matches!(app.mode, AppMode::FilteringScopes(_)));
}

#[test]
fn palette_move_task_up_executes() {
    let queue = make_queue(vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")]);
    let mut app = App::new(queue);
    app.mode = AppMode::CommandPalette {
        query: input("move selected task up"),
        selected: 0,
    };
    app.selected = 1; // Select RQ-0002

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert!(app.dirty);
}
