//! Tests for quit and discard flows.
//!
//! Responsibilities:
//! - Test quit behavior with and without runner active.
//! - Test confirm quit dialog.
//! - Test unsafe-to-discard detection.
//! - Test reload with dirty state confirmation.
//!
//! Does NOT handle:
//! - Other mode transitions (see modes.rs).
//! - Palette commands (see palette.rs).

use super::helpers::{key_event, make_queue, make_test_task};
use crate::tui::app_palette_ops::PaletteOperations;
use crate::tui::events::handle_key_event;
use crate::tui::events::types::ConfirmDiscardAction;
use crate::tui::{App, AppMode, PaletteCommand, TuiAction};
use crossterm::event::KeyCode;

#[test]
fn quit_when_not_running_exits_immediately() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('q')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Quit);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn quit_when_running_requires_confirmation() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.runner_active = true;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('q')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::ConfirmQuit);
}

#[test]
fn confirm_quit_accepts_yes() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmQuit;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Quit);
}

#[test]
fn unsafe_to_discard_detects_dirty_states() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    assert!(!app.unsafe_to_discard());

    app.dirty = true;
    assert!(app.unsafe_to_discard());
    app.dirty = false;

    app.dirty_done = true;
    assert!(app.unsafe_to_discard());
    app.dirty_done = false;

    app.dirty_config = true;
    assert!(app.unsafe_to_discard());
    app.dirty_config = false;

    app.save_error = Some("save failed".to_string());
    assert!(app.unsafe_to_discard());
}

#[test]
fn reload_when_dirty_requires_confirm_discard() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.dirty = true;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('r')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(
        app.mode,
        AppMode::ConfirmDiscard {
            action: ConfirmDiscardAction::ReloadQueue
        }
    );
}

#[test]
fn confirm_discard_reload_yes_triggers_reload() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDiscard {
        action: ConfirmDiscardAction::ReloadQueue,
    };

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::ReloadQueue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_discard_cancel_returns_to_normal() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDiscard {
        action: ConfirmDiscardAction::Quit,
    };

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-19T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn quit_when_dirty_requires_confirm_discard() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.dirty = true;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('q')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(
        app.mode,
        AppMode::ConfirmDiscard {
            action: ConfirmDiscardAction::Quit
        }
    );
}

#[test]
fn palette_reload_with_save_error_requires_confirm_discard() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.save_error = Some("save failed".to_string());

    let action = app
        .execute_palette_command(PaletteCommand::ReloadQueue, "2026-01-19T00:00:00Z")
        .expect("execute command");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(
        app.mode,
        AppMode::ConfirmDiscard {
            action: ConfirmDiscardAction::ReloadQueue
        }
    );
}
