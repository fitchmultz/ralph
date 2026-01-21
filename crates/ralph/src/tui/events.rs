//! TUI event handling extracted from `crate::tui`.
//!
//! This module contains all key-event dispatch and per-mode handlers.
//! Public API is preserved via `crate::tui` re-exporting:
//! - `AppMode`
//! - `TuiAction`
//! - `handle_key_event`
//!
//! This is a pure refactor: behavior must remain identical to the prior
//! inline implementation in `tui.rs`.

use anyhow::Result;
use crossterm::event::KeyCode;

use super::App;

/// Actions that can result from handling a key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    /// Continue running the TUI
    Continue,
    /// Exit the TUI
    Quit,
    /// Run a specific task (transitions to Executing mode)
    RunTask(String),
}

/// Interaction modes for the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    /// Normal navigation mode
    Normal,
    /// Editing task title
    EditingTitle(String),
    /// Confirming task deletion
    ConfirmDelete,
    /// Executing a task (live output view)
    Executing { task_id: String },
}

/// Handle a key event and return the resulting action.
///
/// This function is the core of TUI interaction handling and is public
/// to allow testing without a full terminal setup.
pub fn handle_key_event(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match app.mode.clone() {
        AppMode::Normal => handle_normal_mode_key(app, key, now_rfc3339),
        AppMode::EditingTitle(ref current) => {
            handle_editing_mode_key(app, key, current, now_rfc3339)
        }
        AppMode::ConfirmDelete => handle_confirm_delete_key(app, key),
        AppMode::Executing { .. } => handle_executing_mode_key(app, key),
    }
}

/// Handle key events in Normal mode.
fn handle_normal_mode_key(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => Ok(TuiAction::Quit),
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_up();
            Ok(TuiAction::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let list_height = app.list_height;
            app.move_down(list_height);
            Ok(TuiAction::Continue)
        }
        KeyCode::Enter => {
            if let Some(task) = app.selected_task() {
                let task_id = task.id.clone();
                app.mode = AppMode::Executing {
                    task_id: task_id.clone(),
                };
                app.logs.clear();
                app.log_scroll = 0;
                Ok(TuiAction::RunTask(task_id))
            } else {
                Ok(TuiAction::Continue)
            }
        }
        KeyCode::Char('d') => {
            if app.selected_task().is_some() {
                app.mode = AppMode::ConfirmDelete;
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('e') => {
            if let Some(task) = app.selected_task() {
                app.mode = AppMode::EditingTitle(task.title.clone());
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('s') => {
            let _ = app.cycle_status(now_rfc3339);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in EditingTitle mode.
fn handle_editing_mode_key(
    app: &mut App,
    key: KeyCode,
    current: &str,
    _now_rfc3339: &str,
) -> Result<TuiAction> {
    match key {
        KeyCode::Enter => {
            let new_title = current.to_string();
            let _ = app.update_title(new_title);
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(c) => {
            let mut new_title = current.to_string();
            new_title.push(c);
            app.mode = AppMode::EditingTitle(new_title);
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut new_title = current.to_string();
            new_title.pop();
            app.mode = AppMode::EditingTitle(new_title);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmDelete mode.
fn handle_confirm_delete_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let _ = app.delete_selected_task();
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in Executing mode.
fn handle_executing_mode_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}
