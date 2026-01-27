//! Filter input key handling for the TUI.
//!
//! Responsibilities:
//! - Accept tag/scope filter input and apply changes to `App`.
//! - Handle submit and cancel flow for filter inputs.
//!
//! Not handled here:
//! - Rendering of filter prompts or validation beyond parsing.
//! - Shortcut handling outside filter modes.
//!
//! Invariants/assumptions:
//! - Input is treated as plain text; Ctrl/Alt-modified chars are ignored.
//! - On submit or cancel, the mode returns to Normal.

use super::super::AppMode;
use super::types::TuiAction;
use super::{text_char, App};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in FilteringTags mode.
pub(super) fn handle_filtering_tags_key(
    app: &mut App,
    key: KeyEvent,
    current: &str,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            let tags = App::parse_tags(current);
            app.set_tag_filters(tags);
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
            app.mode = AppMode::FilteringTags(next);
            Ok(TuiAction::Continue)
        }
        _ => {
            if let Some(ch) = text_char(&key) {
                let mut next = current.to_string();
                next.push(ch);
                app.mode = AppMode::FilteringTags(next);
            }
            Ok(TuiAction::Continue)
        }
    }
}

pub(super) fn handle_filtering_scopes_key(
    app: &mut App,
    key: KeyEvent,
    current: &str,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            let scopes = App::parse_list(current);
            app.set_scope_filters(scopes);
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
            app.mode = AppMode::FilteringScopes(next);
            Ok(TuiAction::Continue)
        }
        _ => {
            if let Some(ch) = text_char(&key) {
                let mut next = current.to_string();
                next.push(ch);
                app.mode = AppMode::FilteringScopes(next);
            }
            Ok(TuiAction::Continue)
        }
    }
}
