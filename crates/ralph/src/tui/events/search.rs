//! Search input key handling for the TUI.
//!
//! Responsibilities:
//! - Capture search query input and apply it to filters.
//! - Exit search mode on submit or cancel.
//!
//! Not handled here:
//! - Rendering search UI.
//! - Regex validation or search execution details.
//!
//! Invariants/assumptions:
//! - Search input ignores Ctrl/Alt-modified characters.

use super::super::AppMode;
use super::types::TuiAction;
use super::{text_char, App};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Searching mode.
pub(super) fn handle_searching_mode_key(
    app: &mut App,
    key: KeyEvent,
    current: &str,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            app.set_search_query(current.to_string());
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut next = current.to_string();
            next.pop();
            app.mode = AppMode::Searching(next);
            Ok(TuiAction::Continue)
        }
        _ => {
            if let Some(ch) = text_char(&key) {
                let mut next = current.to_string();
                next.push(ch);
                app.mode = AppMode::Searching(next);
            }
            Ok(TuiAction::Continue)
        }
    }
}
