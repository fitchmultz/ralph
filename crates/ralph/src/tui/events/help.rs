//! Help overlay key handling for the TUI.
//!
//! Responsibilities:
//! - Exit the help overlay on expected keys.
//!
//! Not handled here:
//! - Rendering the help view.
//! - Any navigation outside the help overlay.
//!
//! Invariants/assumptions:
//! - Only plain (non-Ctrl/Alt) characters should close the overlay.

use super::super::AppMode;
use super::types::TuiAction;
use super::{is_plain_char, App};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Help mode.
pub(super) fn handle_help_mode_key(app: &mut App, key: KeyEvent) -> Result<TuiAction> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('?') if is_plain_char(&key, '?') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('h') if is_plain_char(&key, 'h') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}
