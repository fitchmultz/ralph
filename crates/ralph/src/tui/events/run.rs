//! Executing-mode key handling for the TUI.
//!
//! Responsibilities:
//! - Navigate and control the live execution log view.
//! - Exit back to Normal mode when requested.
//!
//! Not handled here:
//! - Rendering execution output.
//! - Runner lifecycle management beyond local state toggles.
//!
//! Invariants/assumptions:
//! - Key handling is stateless and uses current `App` log metadata.

use super::{App, is_plain_char, types::TuiAction};
use crate::tui::app_logs::LogOperations;
use crate::tui::app_scroll::ScrollOperations;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Executing mode.
pub(super) fn handle_executing_mode_key(app: &mut App, key: KeyEvent) -> Result<TuiAction> {
    let visible_lines = app.log_visible_lines();
    let page_lines = visible_lines.saturating_sub(1).max(1);
    match key.code {
        KeyCode::Esc => {
            app.mode = super::super::AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Up => {
            app.scroll_logs_up(1);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('k') if is_plain_char(&key, 'k') => {
            app.scroll_logs_up(1);
            Ok(TuiAction::Continue)
        }
        KeyCode::Down => {
            app.scroll_logs_down(1, visible_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('j') if is_plain_char(&key, 'j') => {
            app.scroll_logs_down(1, visible_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::PageUp => {
            app.scroll_logs_up(page_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::PageDown => {
            app.scroll_logs_down(page_lines, visible_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('a') if is_plain_char(&key, 'a') => {
            if app.autoscroll {
                app.autoscroll = false;
            } else {
                app.enable_autoscroll(visible_lines);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('l') if is_plain_char(&key, 'l') => {
            if app.loop_active {
                app.loop_active = false;
                app.loop_arm_after_current = false;
                app.set_status_message("Loop stopped");
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('p') if is_plain_char(&key, 'p') => {
            app.show_progress_panel = !app.show_progress_panel;
            let status = if app.show_progress_panel {
                "shown"
            } else {
                "hidden"
            };
            app.set_status_message(format!("Progress panel {}", status));
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('f') if is_plain_char(&key, 'f') => {
            let previous = app.mode.clone();
            app.mode = super::super::AppMode::FlowchartOverlay {
                previous_mode: Box::new(previous),
            };
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}
