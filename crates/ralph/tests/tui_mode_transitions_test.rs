//! Contract tests for TUI mode transitions.
//!
//! Responsibilities:
//! - Verify `AppMode` transitions caused by key events.
//! - Confirm mode changes occur without relying on rendering.
//!
//! Not handled here:
//! - Rendering output or terminal backend integration.
//! - Queue persistence or runner side effects.
//!
//! Invariants/assumptions:
//! - Tests use synthetic key events against in-memory queues.

mod test_support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ralph::tui::{self, App, AppMode, MultiLineInput, TuiAction};
use test_support::make_test_queue;

fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn test_mode_transition_normal_to_editing() {
    let mut app = App::new(make_test_queue());
    assert_eq!(app.mode, AppMode::Normal);

    let _ = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('e')),
        "2026-01-19T00:00:00Z",
    )
    .unwrap();

    assert!(matches!(app.mode, AppMode::EditingTask { .. }));
}

#[test]
fn test_mode_transition_normal_to_delete() {
    let mut app = App::new(make_test_queue());
    assert_eq!(app.mode, AppMode::Normal);

    let _ = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('d')),
        "2026-01-19T00:00:00Z",
    )
    .unwrap();

    assert_eq!(app.mode, AppMode::ConfirmDelete);
}

#[test]
fn test_mode_transition_normal_to_executing() {
    let mut app = App::new(make_test_queue());
    assert_eq!(app.mode, AppMode::Normal);

    let _ =
        tui::handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-19T00:00:00Z").unwrap();

    assert!(matches!(app.mode, AppMode::Executing { .. }));
}

#[test]
fn test_mode_transition_editing_to_list_on_save() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some(MultiLineInput::new("New Title", false)),
    };

    let _ =
        tui::handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-19T00:00:00Z").unwrap();

    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: None
        }
    ));
}

#[test]
fn test_mode_transition_editing_to_list_on_cancel() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some(MultiLineInput::new("New Title", false)),
    };

    let _ =
        tui::handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-19T00:00:00Z").unwrap();

    assert!(matches!(
        app.mode,
        AppMode::EditingTask {
            selected: 0,
            editing_value: None
        }
    ));
}

#[test]
fn test_mode_transition_delete_to_normal_on_confirm() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::ConfirmDelete;

    let _ = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-19T00:00:00Z",
    )
    .unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_mode_transition_delete_to_normal_on_cancel() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::ConfirmDelete;

    let _ = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('n')),
        "2026-01-19T00:00:00Z",
    )
    .unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_mode_transition_executing_to_normal() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };

    let _ =
        tui::handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-19T00:00:00Z").unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_enter_key_in_executing_mode_does_not_quit() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };

    let action =
        tui::handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-19T00:00:00Z").unwrap();

    // Should continue, not quit
    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(app.mode, AppMode::Executing { .. }));
}

// Tests for Repair/Unlock palette commands (RQ-0489)

#[test]
fn test_mode_transition_palette_to_confirm_repair() {
    use ralph::tui::PaletteCommand;

    let mut app = App::new(make_test_queue());
    app.queue_path = Some(std::path::PathBuf::from("/tmp/.ralph/queue.json"));
    app.done_path = Some(std::path::PathBuf::from("/tmp/.ralph/done.json"));

    // Execute RepairQueue palette command
    let _ = app.execute_palette_command(PaletteCommand::RepairQueue, "2026-01-19T00:00:00Z");

    assert!(matches!(
        app.mode,
        AppMode::ConfirmRepair { dry_run: false }
    ));
}

#[test]
fn test_mode_transition_palette_to_confirm_repair_dry_run() {
    use ralph::tui::PaletteCommand;

    let mut app = App::new(make_test_queue());
    app.queue_path = Some(std::path::PathBuf::from("/tmp/.ralph/queue.json"));
    app.done_path = Some(std::path::PathBuf::from("/tmp/.ralph/done.json"));

    // Execute RepairQueueDryRun palette command
    let _ = app.execute_palette_command(PaletteCommand::RepairQueueDryRun, "2026-01-19T00:00:00Z");

    assert!(matches!(app.mode, AppMode::ConfirmRepair { dry_run: true }));
}

#[test]
fn test_mode_transition_palette_to_confirm_unlock() {
    use ralph::tui::PaletteCommand;

    let mut app = App::new(make_test_queue());
    app.queue_path = Some(std::path::PathBuf::from("/tmp/.ralph/queue.json"));
    app.done_path = Some(std::path::PathBuf::from("/tmp/.ralph/done.json"));

    // Execute UnlockQueue palette command
    let _ = app.execute_palette_command(PaletteCommand::UnlockQueue, "2026-01-19T00:00:00Z");

    assert_eq!(app.mode, AppMode::ConfirmUnlock);
}

#[test]
fn test_mode_transition_confirm_repair_to_normal_on_confirm() {
    let mut app = App::new(make_test_queue());
    app.queue_path = Some(std::path::PathBuf::from("/tmp/.ralph/queue.json"));
    app.done_path = Some(std::path::PathBuf::from("/tmp/.ralph/done.json"));
    app.mode = AppMode::ConfirmRepair { dry_run: true };

    let _ = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-19T00:00:00Z",
    )
    .unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_mode_transition_confirm_repair_to_normal_on_cancel() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::ConfirmRepair { dry_run: false };

    let _ = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('n')),
        "2026-01-19T00:00:00Z",
    )
    .unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_mode_transition_confirm_unlock_to_normal_on_confirm() {
    let mut app = App::new(make_test_queue());
    app.queue_path = Some(std::path::PathBuf::from("/tmp/.ralph/queue.json"));
    app.mode = AppMode::ConfirmUnlock;

    let _ = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-19T00:00:00Z",
    )
    .unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_mode_transition_confirm_unlock_to_normal_on_cancel() {
    let mut app = App::new(make_test_queue());
    app.mode = AppMode::ConfirmUnlock;

    let _ = tui::handle_key_event(
        &mut app,
        key_event(KeyCode::Char('n')),
        "2026-01-19T00:00:00Z",
    )
    .unwrap();

    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn test_palette_entries_include_repair_and_unlock() {
    let app = App::new(make_test_queue());
    let entries = app.palette_entries("");

    let has_repair = entries.iter().any(|e| e.title == "Repair queue");
    let has_repair_dry_run = entries.iter().any(|e| e.title == "Repair queue (dry run)");
    let has_unlock = entries.iter().any(|e| e.title == "Unlock queue");

    assert!(has_repair, "Palette should include 'Repair queue'");
    assert!(
        has_repair_dry_run,
        "Palette should include 'Repair queue (dry run)'"
    );
    assert!(has_unlock, "Palette should include 'Unlock queue'");
}
