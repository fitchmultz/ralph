//! TUI event handling extracted from `crate::tui`.
//!
//! This module contains all key-event dispatch and per-mode handlers.
//! Public API is preserved via `crate::tui` re-exporting:
//! - `AppMode`
//! - `TuiAction`
//! - `handle_key_event`
//!
//! The interaction model is intentionally user-centric:
//! - `:` opens a command palette (discoverability)
//! - `l` toggles loop mode (auto-run tasks)
//! - `a` archives terminal tasks (done/rejected) with confirmation
//! - `?`/`h` shows the help overlay

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::App;

pub mod confirm;
pub mod create;
pub mod editing;
pub mod filter;
pub mod help;
pub mod normal;
pub mod palette;
pub mod run;
pub mod scan;
pub mod search;
pub mod types;

pub use palette::{PaletteCommand, PaletteEntry};
pub use types::{AppMode, ConfirmDiscardAction, TuiAction};

#[cfg(test)]
mod tests;

/// Handle a key event and return the resulting action.
///
/// This function is the core of TUI interaction handling and is public
/// to allow testing without a full terminal setup.
pub fn handle_key_event(
    app: &mut App,
    key: KeyEvent,
    now_rfc3339: &str,
) -> anyhow::Result<TuiAction> {
    match app.mode.clone() {
        AppMode::Normal => normal::handle_normal_mode_key(app, key, now_rfc3339),
        AppMode::Help => help::handle_help_mode_key(app, key),
        AppMode::EditingTask {
            selected,
            editing_value,
        } => editing::handle_editing_task_key(app, key, selected, editing_value, now_rfc3339),
        AppMode::CreatingTask(ref current) => {
            create::handle_creating_mode_key(app, key, current, now_rfc3339)
        }
        AppMode::CreatingTaskDescription(ref current) => {
            create::handle_creating_description_mode_key(app, key, current)
        }
        AppMode::Searching(ref current) => search::handle_searching_mode_key(app, key, current),
        AppMode::FilteringTags(ref current) => filter::handle_filtering_tags_key(app, key, current),
        AppMode::FilteringScopes(ref current) => {
            filter::handle_filtering_scopes_key(app, key, current)
        }
        AppMode::EditingConfig {
            selected,
            editing_value,
        } => editing::handle_editing_config_key(app, key, selected, editing_value),
        AppMode::Scanning(ref current) => scan::handle_scanning_mode_key(app, key, current),
        AppMode::CommandPalette { query, selected } => {
            palette::handle_command_palette_key(app, key, &query, selected, now_rfc3339)
        }
        AppMode::ConfirmDelete => confirm::handle_confirm_delete_key(app, key),
        AppMode::ConfirmArchive => confirm::handle_confirm_archive_key(app, key, now_rfc3339),
        AppMode::ConfirmQuit => confirm::handle_confirm_quit_key(app, key),
        AppMode::ConfirmDiscard { action } => confirm::handle_confirm_discard_key(app, key, action),
        AppMode::ConfirmRevert {
            label,
            allow_proceed,
            selected,
            input,
            reply_sender,
            previous_mode,
        } => {
            let state = confirm::ConfirmRevertState::new(
                label,
                allow_proceed,
                selected,
                input,
                reply_sender,
                *previous_mode,
            );
            confirm::handle_confirm_revert_key(app, key, state)
        }
        AppMode::Executing { .. } => run::handle_executing_mode_key(app, key),
    }
}

pub(super) fn text_char(key: &KeyEvent) -> Option<char> {
    if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match key.code {
        KeyCode::Char(ch) => Some(ch),
        _ => None,
    }
}

pub(super) fn is_ctrl_char(key: &KeyEvent, expected: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(expected)
}

pub(super) fn is_plain_char(key: &KeyEvent, expected: char) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return false;
    }
    key.code == KeyCode::Char(expected)
}
