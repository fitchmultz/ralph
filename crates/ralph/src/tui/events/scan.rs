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
//! - Scan input ignores Ctrl/Alt-modified characters.

use super::super::AppMode;
use super::types::TuiAction;
use super::{text_char, App};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Scanning mode.
pub(super) fn handle_scanning_mode_key(
    app: &mut App,
    key: KeyEvent,
    current: &str,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            if app.runner_active {
                app.set_status_message("Runner already active");
                return Ok(TuiAction::Continue);
            }
            let focus = current.trim().to_string();
            app.mode = AppMode::Normal;
            Ok(TuiAction::RunScan(focus))
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut next = current.to_string();
            next.pop();
            app.mode = AppMode::Scanning(next);
            Ok(TuiAction::Continue)
        }
        _ => {
            if let Some(ch) = text_char(&key) {
                let mut next = current.to_string();
                next.push(ch);
                app.mode = AppMode::Scanning(next);
            }
            Ok(TuiAction::Continue)
        }
    }
}
