//! Flowchart overlay key event handling.
//!
//! Responsibilities:
//! - Handle keyboard input when the flowchart overlay is active.
//! - Support closing the overlay to return to the previous mode.
//!
//! Not handled here:
//! - Rendering the flowchart view (see `tui::render::overlays`).
//! - Any navigation outside the flowchart overlay.
//!
//! Invariants/assumptions:
//! - Only plain (non-Ctrl/Alt) characters should close the overlay.

use super::types::{AppMode, TuiAction};
use super::{App, is_plain_char};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Flowchart overlay mode.
pub(super) fn handle_flowchart_mode_key(app: &mut App, key: KeyEvent) -> Result<TuiAction> {
    match key.code {
        // Close overlay: Esc, 'f', 'h', or '?' return to previous mode
        KeyCode::Esc => {
            exit_flowchart_mode(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('f') if is_plain_char(&key, 'f') => {
            exit_flowchart_mode(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('?') if is_plain_char(&key, '?') => {
            exit_flowchart_mode(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('h') if is_plain_char(&key, 'h') => {
            exit_flowchart_mode(app);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Exit flowchart mode and restore the previous application mode.
fn exit_flowchart_mode(app: &mut App) {
    if let AppMode::FlowchartOverlay { previous_mode } = &app.mode {
        app.mode = *previous_mode.clone();
    }
}
