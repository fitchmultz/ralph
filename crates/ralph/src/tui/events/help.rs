//! Help overlay key handling for the TUI.
//!
//! Responsibilities:
//! - Exit the help overlay on expected keys.
//! - Support scrolling through help content.
//!
//! Not handled here:
//! - Rendering the help view.
//! - Any navigation outside the help overlay.
//!
//! Invariants/assumptions:
//! - Only plain (non-Ctrl/Alt) characters should close the overlay.

use super::types::TuiAction;
use super::{App, is_plain_char};
use crate::tui::app_scroll::ScrollOperations;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Help mode.
pub(super) fn handle_help_mode_key(app: &mut App, key: KeyEvent) -> Result<TuiAction> {
    let total_lines = app.help_total_lines();
    let page_lines = app.help_visible_lines();

    match key.code {
        KeyCode::Esc => {
            app.exit_help_mode();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('?') if is_plain_char(&key, '?') => {
            app.exit_help_mode();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('h') if is_plain_char(&key, 'h') => {
            app.exit_help_mode();
            Ok(TuiAction::Continue)
        }
        KeyCode::Up => {
            app.scroll_help_up(1);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('k') if is_plain_char(&key, 'k') => {
            app.scroll_help_up(1);
            Ok(TuiAction::Continue)
        }
        KeyCode::Down => {
            app.scroll_help_down(1, total_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('j') if is_plain_char(&key, 'j') => {
            app.scroll_help_down(1, total_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::PageUp => {
            app.scroll_help_up(page_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::PageDown => {
            app.scroll_help_down(page_lines, total_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::Home => {
            app.scroll_help_top();
            Ok(TuiAction::Continue)
        }
        KeyCode::End => {
            app.scroll_help_bottom(total_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('g') if is_plain_char(&key, 'g') => {
            app.scroll_help_top();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('G') if is_plain_char(&key, 'G') => {
            app.scroll_help_bottom(total_lines);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}
