//! Tests for confirm revert dialog functionality.
//!
//! Responsibilities:
//! - Test confirm revert dialog navigation.
//! - Test revert/keep selection.
//! - Test keyboard handling in dialog.
//!
//! Does NOT handle:
//! - Other dialogs or modes (see their respective test modules).

use super::helpers::{ctrl_key_event, key_event, make_queue, make_test_task};
use crate::runutil::RevertDecision;
use crate::tui::TextInput;
use crate::tui::events::confirm::ConfirmRevertState;
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, TuiAction};
use crossterm::event::KeyCode;
use std::sync::mpsc;

fn make_confirm_revert_state(
    selected: usize,
    allow_proceed: bool,
) -> (ConfirmRevertState, mpsc::Receiver<RevertDecision>) {
    let (tx, rx) = mpsc::channel();
    let state = ConfirmRevertState::new(
        "test".to_string(),
        None,
        allow_proceed,
        selected,
        TextInput::new(""),
        tx,
        AppMode::Normal,
    );
    (state, rx)
}

#[test]
fn confirm_revert_j_key_navigates_down() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(0, false);
    app.mode = state.into_mode();

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('j')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(selected, 1),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}

#[test]
fn confirm_revert_k_key_navigates_up() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(2, false);
    app.mode = state.into_mode();

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('k')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(selected, 1),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}

#[test]
fn confirm_revert_down_key_navigates_down() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(0, false);
    app.mode = state.into_mode();

    let action = handle_key_event(&mut app, key_event(KeyCode::Down), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(selected, 1),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}

#[test]
fn confirm_revert_up_key_navigates_up() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(2, false);
    app.mode = state.into_mode();

    let action = handle_key_event(&mut app, key_event(KeyCode::Up), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(selected, 1),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}

#[test]
fn confirm_revert_jk_with_modifiers_ignored() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(1, false);
    app.mode = state.into_mode();

    // Ctrl+j should NOT navigate
    let action = handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('j')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(*selected, 1),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }

    // Alt+k should NOT navigate
    let alt_k =
        crossterm::event::KeyEvent::new(KeyCode::Char('k'), crossterm::event::KeyModifiers::ALT);
    let action = handle_key_event(&mut app, alt_k, "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(*selected, 1),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}

#[test]
fn confirm_revert_navigation_clamps_at_top() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(0, false);
    app.mode = state.into_mode();

    // k at top should stay at 0
    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('k')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(selected, 0),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}

#[test]
fn confirm_revert_navigation_clamps_at_bottom() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(2, false);
    app.mode = state.into_mode();

    // j at bottom (index 2 for 3 options) should stay at 2
    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('j')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(selected, 2),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}

#[test]
fn confirm_revert_enter_selects_keep() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, rx) = make_confirm_revert_state(0, false);
    app.mode = state.into_mode();

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(rx.try_recv().unwrap(), RevertDecision::Keep);
}

#[test]
fn confirm_revert_enter_selects_revert() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, rx) = make_confirm_revert_state(1, false);
    app.mode = state.into_mode();

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(rx.try_recv().unwrap(), RevertDecision::Revert);
}

#[test]
fn confirm_revert_esc_defaults_to_keep() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, rx) = make_confirm_revert_state(2, false);
    app.mode = state.into_mode();

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(rx.try_recv().unwrap(), RevertDecision::Keep);
}

#[test]
fn confirm_revert_mixed_navigation_j_then_down() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(0, false);
    app.mode = state.into_mode();

    // j then Down should work correctly
    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('j')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    handle_key_event(&mut app, key_event(KeyCode::Down), "2026-01-20T00:00:00Z")
        .expect("handle key");

    match app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(selected, 2),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}

#[test]
fn confirm_revert_mixed_navigation_k_then_up() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let (state, _rx) = make_confirm_revert_state(2, false);
    app.mode = state.into_mode();

    // k then Up should work correctly
    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('k')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    handle_key_event(&mut app, key_event(KeyCode::Up), "2026-01-20T00:00:00Z").expect("handle key");

    match app.mode {
        AppMode::ConfirmRevert { selected, .. } => assert_eq!(selected, 0),
        other => panic!("expected ConfirmRevert mode, got {:?}", other),
    }
}
