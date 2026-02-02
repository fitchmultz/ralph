//! Tests for search and filter functionality.
//!
//! Responsibilities:
//! - Test search input handling.
//! - Test tag filter functionality.
//! - Test scope filter functionality.
//! - Test filter live preview.
//! - Test filter restoration on cancel.
//!
//! Does NOT handle:
//! - Navigation behavior (see navigation.rs).
//! - Mode transitions (see modes.rs).

use super::helpers::{ctrl_key_event, input, input_with_cursor, key_event, make_queue};
use crate::tui::app_filters::FilterManagementOperations;
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, TuiAction};
use crossterm::event::KeyCode;

fn assert_search_input(app: &App, expected: &str, cursor: usize) {
    match &app.mode {
        AppMode::Searching(query) => {
            assert_eq!(query.value(), expected);
            assert_eq!(query.cursor(), cursor);
        }
        other => panic!("expected search mode, got {:?}", other),
    }
}

#[test]
fn search_enter_applies_query_and_returns_to_normal() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::Searching(input("needle"));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.filters.query, "needle");
}

#[test]
fn search_escape_restores_previous_query_after_live_preview() {
    let mut app = App::new(make_queue(vec![]));
    app.filters.query = "prior".to_string();

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('/')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(matches!(app.mode, AppMode::Searching(_)));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('X')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(app.filters.query, "priorX");

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.filters.query, "prior");
}

#[test]
fn search_input_supports_cursor_edits_and_deletes() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::Searching(input_with_cursor("ac", 1));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('b')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_search_input(&app, "abc", 2);

    handle_key_event(&mut app, key_event(KeyCode::Left), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_search_input(&app, "abc", 1);

    handle_key_event(&mut app, key_event(KeyCode::Delete), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_search_input(&app, "ac", 1);

    handle_key_event(
        &mut app,
        key_event(KeyCode::Backspace),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_search_input(&app, "c", 0);
}

#[test]
fn search_ctrl_w_deletes_previous_word() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::Searching(input("alpha beta"));

    handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('w')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_search_input(&app, "alpha ", 6);
}

#[test]
fn tag_filter_live_preview_updates_filters() {
    let mut app = App::new(make_queue(vec![]));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('t')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(matches!(app.mode, AppMode::FilteringTags(_)));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('a')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(app.filters.tags, vec!["a"]);
}

#[test]
fn tag_filter_escape_restores_previous_filters() {
    let mut app = App::new(make_queue(vec![]));
    app.set_tag_filters(vec!["alpha".to_string()]);

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('t')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(matches!(app.mode, AppMode::FilteringTags(_)));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('b')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(app.filters.tags, vec!["alphab"]);

    handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.filters.tags, vec!["alpha"]);
}

#[test]
fn scope_filter_live_preview_updates_filters() {
    let mut app = App::new(make_queue(vec![]));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('o')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(matches!(app.mode, AppMode::FilteringScopes(_)));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('c')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(app.filters.search_options.scopes, vec!["c"]);
}

#[test]
fn scope_filter_escape_restores_previous_filters() {
    let mut app = App::new(make_queue(vec![]));
    app.set_scope_filters(vec!["docs".to_string()]);

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('o')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(matches!(app.mode, AppMode::FilteringScopes(_)));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('x')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(app.filters.search_options.scopes, vec!["docsx"]);

    handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.filters.search_options.scopes, vec!["docs"]);
}
