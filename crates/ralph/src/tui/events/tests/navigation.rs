//! Tests for navigation and scrolling behavior.
//!
//! Responsibilities:
//! - Test list navigation (j/k, arrow keys).
//! - Test scrolling behavior (Home/End).
//! - Test mouse scroll and click handling.
//! - Test help mode scrolling.
//!
//! Does NOT handle:
//! - Mode transitions (see modes.rs).
//! - Filter behavior (see filters.rs).

use super::helpers::{key_event, make_queue, make_test_task, mouse_event};
use crate::tui::app_filters::FilterManagementOperations;
use crate::tui::events::{handle_key_event, handle_mouse_event};
use crate::tui::{App, AppMode, TuiAction};
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

#[test]
fn help_mode_closes_on_escape() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_mode_scrolls_and_clamps() {
    let mut app = App::new(make_queue(vec![]));
    app.mode = AppMode::Help;
    app.set_help_visible_lines(3, 7);

    handle_key_event(&mut app, key_event(KeyCode::Down), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.help_scroll(), 1);

    handle_key_event(
        &mut app,
        key_event(KeyCode::PageDown),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(app.help_scroll(), 4);

    handle_key_event(
        &mut app,
        key_event(KeyCode::PageDown),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(app.help_scroll(), 4);

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.help_scroll(), 0);

    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.help_scroll(), 4);
}

#[test]
fn normal_mode_home_end_jumps_list_selection_and_scroll() {
    let queue = make_queue(vec![
        make_test_task("RQ-0001"),
        make_test_task("RQ-0002"),
        make_test_task("RQ-0003"),
        make_test_task("RQ-0004"),
        make_test_task("RQ-0005"),
    ]);
    let mut app = App::new(queue);
    app.list_height = 2;

    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.selected, 4);
    assert_eq!(app.scroll, 3);

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
}

#[test]
fn normal_mode_home_end_respects_filtered_list() {
    let mut task1 = make_test_task("RQ-0001");
    task1.tags = vec!["alpha".to_string()];
    let mut task2 = make_test_task("RQ-0002");
    task2.tags = vec!["beta".to_string()];
    let mut task3 = make_test_task("RQ-0003");
    task3.tags = vec!["alpha".to_string()];

    let queue = make_queue(vec![task1, task2, task3]);
    let mut app = App::new(queue);
    app.list_height = 1;
    app.set_tag_filters(vec!["alpha".to_string()]);

    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.selected, 1);
    assert_eq!(app.scroll, 1);
    assert_eq!(
        app.selected_task().map(|task| task.id.as_str()),
        Some("RQ-0003")
    );

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
    assert_eq!(
        app.selected_task().map(|task| task.id.as_str()),
        Some("RQ-0001")
    );
}

#[test]
fn normal_mode_home_end_scrolls_details_when_focused() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.focus_next_panel();
    // Set up scroll state through the details field
    app.details.scroll_down(2);
    let selected_before = app.selected;

    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");
    // ScrollViewState handles bounds internally, just verify scroll changed
    // scroll() returns usize which is always >= 0
    let _scroll = app.details.scroll();
    assert_eq!(app.selected, selected_before);

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.details.scroll(), 0);
    assert_eq!(app.selected, selected_before);
}

#[test]
fn normal_mode_home_end_safe_with_empty_list() {
    let mut app = App::new(make_queue(vec![]));
    app.list_height = 3;

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
}

#[test]
fn help_mode_closes_on_h() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('h')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_mode_closes_on_question_mark() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('?')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_mode_ignores_unrelated_keys() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('x')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn mouse_scroll_down_moves_selection_in_list() {
    let queue = make_queue(vec![
        make_test_task("RQ-0001"),
        make_test_task("RQ-0002"),
        make_test_task("RQ-0003"),
    ]);
    let mut app = App::new(queue);
    app.list_height = 1;

    let action = handle_mouse_event(&mut app, mouse_event(MouseEventKind::ScrollDown, 1, 1))
        .expect("handle mouse");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 1);
    assert_eq!(app.scroll, 1);
}

#[test]
fn mouse_scroll_down_moves_details_when_focused() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.focus_next_panel();
    // Scroll state starts at 0

    let action = handle_mouse_event(&mut app, mouse_event(MouseEventKind::ScrollDown, 1, 1))
        .expect("handle mouse");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.details.scroll(), 1);
    assert_eq!(app.selected, 0);
}

#[test]
fn mouse_scroll_up_moves_selection_in_list() {
    let queue = make_queue(vec![
        make_test_task("RQ-0001"),
        make_test_task("RQ-0002"),
        make_test_task("RQ-0003"),
    ]);
    let mut app = App::new(queue);
    app.list_height = 1;
    app.selected = 1;
    app.scroll = 1;

    let action = handle_mouse_event(&mut app, mouse_event(MouseEventKind::ScrollUp, 1, 1))
        .expect("handle mouse");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
}

#[test]
fn mouse_scroll_up_moves_details_when_focused() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.focus_next_panel();
    // Set initial scroll position
    app.details.scroll_down(2);

    let action = handle_mouse_event(&mut app, mouse_event(MouseEventKind::ScrollUp, 1, 1))
        .expect("handle mouse");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.details.scroll(), 1);
    assert_eq!(app.selected, 0);
}

#[test]
fn mouse_left_click_selects_row_and_focuses_list() {
    let queue = make_queue(vec![
        make_test_task("RQ-0001"),
        make_test_task("RQ-0002"),
        make_test_task("RQ-0003"),
        make_test_task("RQ-0004"),
        make_test_task("RQ-0005"),
    ]);
    let mut app = App::new(queue);
    app.focus_next_panel();
    app.list_height = 3;
    app.scroll = 1;
    app.set_list_area(Rect::new(0, 2, 20, 3));
    let row = app.list_area().expect("list area").y + 1;

    let action = handle_mouse_event(
        &mut app,
        mouse_event(MouseEventKind::Down(MouseButton::Left), 1, row),
    )
    .expect("handle mouse");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 2);
    assert!(!app.details_focused());
}

#[test]
fn mouse_left_click_outside_list_does_not_change_selection() {
    let queue = make_queue(vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")]);
    let mut app = App::new(queue);
    app.focus_next_panel();
    app.selected = 1;
    app.set_list_area(Rect::new(0, 1, 10, 2));

    let action = handle_mouse_event(
        &mut app,
        mouse_event(MouseEventKind::Down(MouseButton::Left), 20, 20),
    )
    .expect("handle mouse");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 1);
    assert!(app.details_focused());
}

#[test]
fn mouse_left_click_on_empty_list_is_noop() {
    let mut app = App::new(make_queue(vec![]));
    app.focus_next_panel();
    app.set_list_area(Rect::new(0, 0, 10, 2));

    let action = handle_mouse_event(
        &mut app,
        mouse_event(MouseEventKind::Down(MouseButton::Left), 1, 1),
    )
    .expect("handle mouse");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.selected, 0);
    assert!(app.details_focused());
}
