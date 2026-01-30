//! Tests for task operations.
//!
//! Responsibilities:
//! - Test task movement (up/down in queue).
//! - Test task deletion.
//! - Test task archiving flow.
//! - Test loop execution.
//!
//! Does NOT handle:
//! - Palette command execution (see palette.rs).
//! - Auto-archive behavior (see auto_archive.rs).
//! - Status/priority changes (see status_priority.rs).

use super::helpers::{key_event, make_queue, make_test_task};
use crate::contracts::TaskStatus;
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, TuiAction};
use crossterm::event::KeyCode;

#[test]
fn loop_key_starts_loop_and_runs_next_runnable() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('L')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::RunTask("RQ-0001".to_string()));
    assert!(app.loop_active);
    assert!(app.runner_active);
}

#[test]
fn delete_key_without_selection_sets_status_message() {
    let mut app = App::new(make_queue(vec![]));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('d')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.status_message.as_deref(), Some("No task selected"));
}

#[test]
fn archive_flow_enters_confirm_mode_then_moves_tasks() {
    let mut done_task = make_test_task("RQ-0001");
    done_task.status = TaskStatus::Done;
    done_task.completed_at = Some("2026-01-19T00:00:00Z".to_string());

    let queue = make_queue(vec![done_task, make_test_task("RQ-0002")]);
    let mut app = App::new(queue);

    // Enter confirm archive.
    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('a')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::ConfirmArchive);

    // Confirm.
    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);

    assert_eq!(app.queue.tasks.len(), 1);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert_eq!(app.done.tasks.len(), 1);
    assert_eq!(app.done.tasks[0].id, "RQ-0001");
    assert!(app.dirty);
    assert!(app.dirty_done);
}

#[test]
fn uppercase_k_moves_selected_task_up_in_queue() {
    let queue = make_queue(vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")]);
    let mut app = App::new(queue);
    app.selected = 1; // Select RQ-0002

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('K')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert_eq!(app.queue.tasks[1].id, "RQ-0001");
    assert!(app.dirty);
}

#[test]
fn uppercase_j_moves_selected_task_down_in_queue() {
    let queue = make_queue(vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")]);
    let mut app = App::new(queue);
    app.selected = 0; // Select RQ-0001

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('J')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert_eq!(app.queue.tasks[1].id, "RQ-0001");
    assert!(app.dirty);
}

#[test]
fn move_task_up_at_top_does_not_mutate_queue() {
    let queue = make_queue(vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")]);
    let mut app = App::new(queue);
    app.selected = 0; // Select RQ-0001

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('K')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0001");
    assert_eq!(app.queue.tasks[1].id, "RQ-0002");
    assert!(!app.dirty);
}

#[test]
fn move_task_down_at_bottom_does_not_mutate_queue() {
    let queue = make_queue(vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")]);
    let mut app = App::new(queue);
    app.selected = 1; // Select RQ-0002

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('J')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0001");
    assert_eq!(app.queue.tasks[1].id, "RQ-0002");
    assert!(!app.dirty);
}

#[test]
fn move_task_with_filters_swaps_underlying_queue_indices() {
    let mut t1 = make_test_task("RQ-0001");
    t1.tags = vec!["a".to_string()];
    let t2 = make_test_task("RQ-0002"); // No tag 'a'
    let mut t3 = make_test_task("RQ-0003");
    t3.tags = vec!["a".to_string()];

    let queue = make_queue(vec![t1, t2, t3]);
    let mut app = App::new(queue);
    app.filters.tags = vec!["a".to_string()];
    app.rebuild_filtered_view();

    // Filtered list should be [RQ-0001, RQ-0003] (indices 0 and 2)
    assert_eq!(app.filtered_indices, vec![0, 2]);
    app.selected = 1; // Select RQ-0003

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('K')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    // Should swap RQ-0001 and RQ-0003 (indices 0 and 2)
    assert_eq!(app.queue.tasks[0].id, "RQ-0003");
    assert_eq!(app.queue.tasks[1].id, "RQ-0002"); // Unchanged
    assert_eq!(app.queue.tasks[2].id, "RQ-0001");
    assert!(app.dirty);
    assert_eq!(app.filtered_indices, vec![0, 2]);
    assert_eq!(
        app.selected_task().map(|task| task.id.as_str()),
        Some("RQ-0003")
    );
}
