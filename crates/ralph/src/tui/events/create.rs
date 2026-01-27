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
//! - Text input ignores Ctrl/Alt modified characters.
//! - Callers provide the current input state for editing.

use super::super::AppMode;
use super::types::TuiAction;
use super::{text_char, App};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in CreatingTask mode.
pub(super) fn handle_creating_mode_key(
    app: &mut App,
    key: KeyEvent,
    current: &str,
    now_rfc3339: &str,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            if let Err(e) = app.create_task_from_title(current, now_rfc3339) {
                app.set_status_message(format!("Error: {}", e));
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut new_title = current.to_string();
            new_title.pop();
            app.mode = AppMode::CreatingTask(new_title);
            Ok(TuiAction::Continue)
        }
        _ => {
            if let Some(ch) = text_char(&key) {
                let mut new_title = current.to_string();
                new_title.push(ch);
                app.mode = AppMode::CreatingTask(new_title);
            }
            Ok(TuiAction::Continue)
        }
    }
}

/// Handle key events in CreatingTaskDescription mode.
pub(super) fn handle_creating_description_mode_key(
    app: &mut App,
    key: KeyEvent,
    current: &str,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            let description = current.trim().to_string();
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
        KeyCode::Backspace => {
            let mut new_description = current.to_string();
            new_description.pop();
            app.mode = AppMode::CreatingTaskDescription(new_description);
            Ok(TuiAction::Continue)
        }
        _ => {
            if let Some(ch) = text_char(&key) {
                let mut new_description = current.to_string();
                new_description.push(ch);
                app.mode = AppMode::CreatingTaskDescription(new_description);
            }
            Ok(TuiAction::Continue)
        }
    }
}
