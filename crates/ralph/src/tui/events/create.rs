//! Task creation key handling for the TUI.
//!
//! Responsibilities:
//! - Capture text input for new task titles and task builder descriptions.
//! - Transition back to normal mode or trigger task creation actions.
//!
//! Not handled here:
//! - Validation beyond basic empty checks.
//! - Rendering or palette command execution.
//!
//! Invariants/assumptions:
//! - Cursor-aware text input is provided by `TextInput`.
//! - Callers provide the current input state for editing.

use super::super::input::{TextInputEdit, apply_text_input_key};
use super::super::{AppMode, TextInput};
use super::App;
use super::types::TuiAction;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in CreatingTask mode.
pub(super) fn handle_creating_mode_key(
    app: &mut App,
    key: KeyEvent,
    mut current: TextInput,
    now_rfc3339: &str,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            if let Err(e) = app.create_task_from_title(current.value(), now_rfc3339) {
                app.set_status_message(format!("Error: {}", e));
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => {
            if apply_text_input_key(&mut current, &key) == TextInputEdit::Changed {
                app.mode = AppMode::CreatingTask(current);
            }
            Ok(TuiAction::Continue)
        }
    }
}

/// Handle key events in CreatingTaskDescription mode.
pub(super) fn handle_creating_description_mode_key(
    app: &mut App,
    key: KeyEvent,
    mut current: TextInput,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            let description = current.value().trim().to_string();
            if description.is_empty() {
                app.mode = AppMode::Normal;
                app.set_status_message("Description cannot be empty");
                return Ok(TuiAction::Continue);
            }
            app.mode = AppMode::Normal;
            Ok(TuiAction::BuildTask(description))
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => {
            if apply_text_input_key(&mut current, &key) == TextInputEdit::Changed {
                app.mode = AppMode::CreatingTaskDescription(current);
            }
            Ok(TuiAction::Continue)
        }
    }
}
