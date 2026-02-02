//! Contract tests for TUI list navigation (selection + scrolling).
//!
//! These tests cover `App::move_up` and `App::move_down`, ensuring bounds checking and scroll
//! tracking stay consistent as selection changes.

mod test_support;

use ralph::contracts::{QueueFile, Task, TaskStatus};
use ralph::tui::App;
use ralph::tui::NavigationOperations;
use test_support::{make_test_queue, make_test_task};

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
