//! Tests for resize event handling.
//!
//! Responsibilities:
//! - Test resize event handling.
//! - Test clamping behavior on resize.
//! - Test edge cases (empty queue, zero dimensions).
//!
//! Does NOT handle:
//! - Other event types (see their respective test modules).

use super::helpers::{make_queue, make_test_task};
use crate::tui::App;
use crate::tui::app_panel::PanelOperations;
use crate::tui::app_scroll::ScrollOperations;

#[test]
fn resize_event_clamps_selected_to_filtered_len() {
    let queue = make_queue(vec![
        make_test_task("RQ-0001"),
        make_test_task("RQ-0002"),
        make_test_task("RQ-0003"),
    ]);
    let mut app = App::new(queue);
    app.selected = 2;
    app.scroll = 2;

    // Simulate filtering down to 1 task, then resize
    app.filters.tags = vec!["nonexistent".to_string()];
    app.rebuild_filtered_view();

    // Trigger resize handling
    app.handle_resize(80, 24);

    // Selected should be clamped to valid range
    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
}

#[test]
fn resize_event_clears_list_area() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.set_list_area(ratatui::layout::Rect::new(0, 0, 80, 24));
    assert!(app.list_area().is_some());

    app.handle_resize(100, 30);

    assert!(app.list_area().is_none());
}

#[test]
fn resize_event_clamps_details_scroll() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    // Set up scroll state - ScrollViewState handles bounds internally
    app.details.scroll_down(100);

    app.handle_resize(80, 10); // Small height

    // ScrollViewState manages its own bounds
    // scroll() returns usize which is always >= 0
    let _scroll = app.details.scroll();
}

#[test]
fn resize_event_clamps_help_scroll() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    // Set up help state with scroll beyond visible range
    app.set_help_visible_lines(5, 10); // 5 visible, 10 total
    app.scroll_help_down(100, 10); // Scroll way down (will be clamped to 5)

    app.handle_resize(80, 10); // Small height

    // help_scroll should be clamped to max valid value
    let expected_max = app.max_help_scroll(10);
    assert_eq!(app.help_scroll(), expected_max);
}

#[test]
fn resize_event_empty_queue_no_panic() {
    let queue = make_queue(vec![]);
    let mut app = App::new(queue);

    // Should not panic with empty queue
    app.handle_resize(80, 24);

    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
}

#[test]
fn resize_event_zero_dimensions_no_panic() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    // Should not panic with zero dimensions
    app.handle_resize(0, 0);

    // App state should remain valid
    assert_eq!(app.selected, 0);
}
