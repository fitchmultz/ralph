//! Tests for status and priority commands.
//!
//! Responsibilities:
//! - Test status change commands.
//! - Test priority change commands.
//! - Test edge cases (no selection).
//!
//! Does NOT handle:
//! - Auto-archive behavior triggered by status changes (see auto_archive.rs).
//! - Other palette commands (see palette.rs).

use super::helpers::{make_queue, make_test_task};
use crate::contracts::{TaskPriority, TaskStatus};
use crate::tui::App;
use crate::tui::app_palette_ops::PaletteOperations;
use crate::tui::events::PaletteCommand;

#[test]
fn palette_set_status_draft_updates_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetStatusDraft, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].status, TaskStatus::Draft);
    assert!(app.status_message.as_ref().unwrap().contains("draft"));
    assert!(app.dirty);
}

#[test]
fn palette_set_status_todo_updates_task() {
    let mut task = make_test_task("RQ-0001");
    task.status = TaskStatus::Draft;
    let queue = make_queue(vec![task]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetStatusTodo, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].status, TaskStatus::Todo);
    assert!(app.status_message.as_ref().unwrap().contains("todo"));
}

#[test]
fn palette_set_status_doing_updates_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetStatusDoing, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].status, TaskStatus::Doing);
    assert!(app.status_message.as_ref().unwrap().contains("doing"));
}

#[test]
fn palette_set_status_done_updates_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetStatusDone, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].status, TaskStatus::Done);
    assert!(app.status_message.as_ref().unwrap().contains("done"));
}

#[test]
fn palette_set_status_rejected_updates_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetStatusRejected, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].status, TaskStatus::Rejected);
    assert!(app.status_message.as_ref().unwrap().contains("rejected"));
}

#[test]
fn palette_set_status_no_task_selected() {
    let queue = make_queue(vec![]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetStatusDone, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.status_message.as_deref(), Some("No task selected"));
}

#[test]
fn palette_set_priority_critical_updates_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetPriorityCritical, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].priority, TaskPriority::Critical);
    assert!(app.status_message.as_ref().unwrap().contains("critical"));
}

#[test]
fn palette_set_priority_high_updates_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetPriorityHigh, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].priority, TaskPriority::High);
    assert!(app.status_message.as_ref().unwrap().contains("high"));
}

#[test]
fn palette_set_priority_medium_updates_task() {
    let mut task = make_test_task("RQ-0001");
    task.priority = TaskPriority::Low;
    let queue = make_queue(vec![task]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetPriorityMedium, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].priority, TaskPriority::Medium);
    assert!(app.status_message.as_ref().unwrap().contains("medium"));
}

#[test]
fn palette_set_priority_low_updates_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetPriorityLow, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks[0].priority, TaskPriority::Low);
    assert!(app.status_message.as_ref().unwrap().contains("low"));
}

#[test]
fn palette_set_priority_no_task_selected() {
    let queue = make_queue(vec![]);
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::SetPriorityHigh, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.status_message.as_deref(), Some("No task selected"));
}
