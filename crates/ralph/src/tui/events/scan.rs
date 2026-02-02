//! Scan input key handling for the TUI.
//!
//! Responsibilities:
//! - Capture scan focus input and trigger scan execution.
//! - Handle cancel flow back to Normal mode.
//!
//! Not handled here:
//! - Scan execution lifecycle (handled by runner integration).
//! - Rendering the scanning UI.
//!
//! Invariants/assumptions:
//! - Scan input uses cursor-aware `TextInput` edits.

use super::super::input::{TextInputEdit, apply_text_input_key};
use super::super::{AppMode, TextInput};
use super::App;
use super::types::TuiAction;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Scanning mode.
pub(super) fn handle_scanning_mode_key(
    app: &mut App,
    key: KeyEvent,
    mut current: TextInput,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            if app.runner_active {
                app.set_status_message("Runner already active");
                return Ok(TuiAction::Continue);
            }
            let focus = current.value().trim().to_string();
            app.mode = AppMode::Normal;
            Ok(TuiAction::RunScan(focus))
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => {
            if apply_text_input_key(&mut current, &key) == TextInputEdit::Changed {
                app.mode = AppMode::Scanning(current);
            }
            Ok(TuiAction::Continue)
        }
    }
}
