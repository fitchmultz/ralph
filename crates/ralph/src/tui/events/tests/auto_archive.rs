//! Tests for auto-archive functionality.
//!
//! Responsibilities:
//! - Test auto-archive behavior modes.
//! - Test confirm archive dialog.
//! - Test archive on terminal status.
//!
//! Does NOT handle:
//! - Manual archiving (see task_ops.rs).
//! - Other status changes (see status_priority.rs).

use super::helpers::{key_event, make_queue, make_test_task};
use crate::contracts::{AutoArchiveBehavior, TaskStatus};
use crate::tui::events::PaletteCommand;
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, TuiAction};
use crossterm::event::KeyCode;

#[test]
fn auto_archive_never_does_not_archive_on_terminal_status() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.project_config.tui.auto_archive_terminal = Some(AutoArchiveBehavior::Never);

    app.execute_palette_command(PaletteCommand::SetStatusDone, "2026-01-20T00:00:00Z")
        .expect("execute command");

    // Task should still be in queue
    assert_eq!(app.queue.tasks.len(), 1);
    assert_eq!(app.done.tasks.len(), 0);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn auto_archive_always_archives_immediately_on_terminal_status() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.project_config.tui.auto_archive_terminal = Some(AutoArchiveBehavior::Always);

    app.execute_palette_command(PaletteCommand::SetStatusDone, "2026-01-20T00:00:00Z")
        .expect("execute command");

    // Task should be moved to done
    assert_eq!(app.queue.tasks.len(), 0);
    assert_eq!(app.done.tasks.len(), 1);
    assert_eq!(app.done.tasks[0].id, "RQ-0001");
    assert!(app.status_message.as_ref().unwrap().contains("Archived"));
}

#[test]
fn auto_archive_prompt_enters_confirm_mode_on_terminal_status() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.project_config.tui.auto_archive_terminal = Some(AutoArchiveBehavior::Prompt);

    app.execute_palette_command(PaletteCommand::SetStatusDone, "2026-01-20T00:00:00Z")
        .expect("execute command");

    // Task should still be in queue but mode should be ConfirmAutoArchive
    assert_eq!(app.queue.tasks.len(), 1);
    assert_eq!(app.done.tasks.len(), 0);
    assert!(
        matches!(app.mode, AppMode::ConfirmAutoArchive(ref id) if id == "RQ-0001"),
        "expected ConfirmAutoArchive mode, got {:?}",
        app.mode
    );
}

#[test]
fn auto_archive_prompt_on_rejected_status() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.project_config.tui.auto_archive_terminal = Some(AutoArchiveBehavior::Prompt);

    app.execute_palette_command(PaletteCommand::SetStatusRejected, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(matches!(app.mode, AppMode::ConfirmAutoArchive(ref id) if id == "RQ-0001"));
}

#[test]
fn auto_archive_not_triggered_on_non_terminal_status() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.project_config.tui.auto_archive_terminal = Some(AutoArchiveBehavior::Always);

    // Setting to Todo should not trigger archive even with Always
    app.execute_palette_command(PaletteCommand::SetStatusTodo, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert_eq!(app.queue.tasks.len(), 1);
    assert_eq!(app.done.tasks.len(), 0);
}

// Tests for ConfirmAutoArchive dialog
#[test]
fn confirm_auto_archive_yes_archives_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.project_config.tui.auto_archive_terminal = Some(AutoArchiveBehavior::Prompt);

    // First set status to Done to trigger the prompt
    app.execute_palette_command(PaletteCommand::SetStatusDone, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(matches!(app.mode, AppMode::ConfirmAutoArchive(_)));

    // Confirm with 'y'
    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.queue.tasks.len(), 0);
    assert_eq!(app.done.tasks.len(), 1);
    assert!(app.status_message.as_ref().unwrap().contains("Archived"));
}

#[test]
fn confirm_auto_archive_n_cancels() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.project_config.tui.auto_archive_terminal = Some(AutoArchiveBehavior::Prompt);

    // First set status to Done to trigger the prompt
    app.execute_palette_command(PaletteCommand::SetStatusDone, "2026-01-20T00:00:00Z")
        .expect("execute command");

    // Cancel with 'n'
    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('n')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.queue.tasks.len(), 1); // Task still in queue
    assert_eq!(app.done.tasks.len(), 0);
}

#[test]
fn confirm_auto_archive_esc_cancels() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.project_config.tui.auto_archive_terminal = Some(AutoArchiveBehavior::Prompt);

    // First set status to Done to trigger the prompt
    app.execute_palette_command(PaletteCommand::SetStatusDone, "2026-01-20T00:00:00Z")
        .expect("execute command");

    // Cancel with Esc
    let action =
        handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.queue.tasks.len(), 1);
    assert_eq!(app.done.tasks.len(), 0);
}

#[test]
fn archive_single_task_moves_terminal_task() {
    let mut task = make_test_task("RQ-0001");
    task.status = TaskStatus::Done;
    let queue = make_queue(vec![task]);
    let mut app = App::new(queue);

    app.archive_single_task("RQ-0001", "2026-01-20T00:00:00Z")
        .expect("archive task");

    assert_eq!(app.queue.tasks.len(), 0);
    assert_eq!(app.done.tasks.len(), 1);
    assert_eq!(app.done.tasks[0].id, "RQ-0001");
    assert!(app.dirty);
    assert!(app.dirty_done);
}

#[test]
fn archive_single_task_fails_for_non_terminal() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]); // Todo status
    let mut app = App::new(queue);

    let result = app.archive_single_task("RQ-0001", "2026-01-20T00:00:00Z");

    assert!(result.is_err());
    assert_eq!(app.queue.tasks.len(), 1);
    assert_eq!(app.done.tasks.len(), 0);
}

#[test]
fn archive_single_task_fails_for_missing_task() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let result = app.archive_single_task("RQ-9999", "2026-01-20T00:00:00Z");

    assert!(result.is_err());
}
