//! Normal-mode key handling for the TUI.
//!
//! Responsibilities:
//! - Map single-key shortcuts into TUI actions or mode transitions.
//! - Route shared actions through palette command execution for consistency.
//!
//! Not handled here:
//! - Confirmation dialogs or other modal key handling.
//! - Rendering logic.
//!
//! Invariants/assumptions:
//! - `App` state mutations are safe to perform immediately on key press.
//! - Shared commands should use `execute_palette_command` for unified gating.

use super::super::{AppMode, TextInput};
use super::types::TuiAction;
use super::{is_ctrl_char, is_plain_char, App};
use crate::tui::PaletteCommand;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Normal mode.
pub(super) fn handle_normal_mode_key(
    app: &mut App,
    key: KeyEvent,
    now_rfc3339: &str,
) -> Result<TuiAction> {
    if is_ctrl_char(&key, 'c') || is_ctrl_char(&key, 'q') {
        return app.execute_palette_command(PaletteCommand::Quit, now_rfc3339);
    }
    if is_ctrl_char(&key, 'p') {
        app.mode = AppMode::CommandPalette {
            query: TextInput::new(""),
            selected: 0,
        };
        return Ok(TuiAction::Continue);
    }
    if is_ctrl_char(&key, 'f') {
        app.start_search_input();
        return Ok(TuiAction::Continue);
    }

    match key.code {
        KeyCode::Char(':') if is_plain_char(&key, ':') => {
            app.mode = AppMode::CommandPalette {
                query: TextInput::new(""),
                selected: 0,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('q') if is_plain_char(&key, 'q') => {
            app.execute_palette_command(PaletteCommand::Quit, now_rfc3339)
        }
        KeyCode::Esc => app.execute_palette_command(PaletteCommand::Quit, now_rfc3339),
        KeyCode::Tab => {
            app.focus_next_panel();
            Ok(TuiAction::Continue)
        }
        KeyCode::BackTab => {
            app.focus_previous_panel();
            Ok(TuiAction::Continue)
        }
        KeyCode::Up => {
            if app.details_focused() {
                app.scroll_details_up(1);
            } else {
                app.move_up();
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('k') if is_plain_char(&key, 'k') => {
            if app.details_focused() {
                app.scroll_details_up(1);
            } else {
                app.move_up();
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('K') if is_plain_char(&key, 'K') => {
            app.execute_palette_command(PaletteCommand::MoveTaskUp, now_rfc3339)
        }
        KeyCode::Down => {
            if app.details_focused() {
                let total_lines = app.details_total_lines;
                app.scroll_details_down(1, total_lines);
            } else {
                let list_height = app.list_height;
                app.move_down(list_height);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('j') if is_plain_char(&key, 'j') => {
            if app.details_focused() {
                let total_lines = app.details_total_lines;
                app.scroll_details_down(1, total_lines);
            } else {
                let list_height = app.list_height;
                app.move_down(list_height);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::PageUp => {
            if app.details_focused() {
                let page_lines = app.details_visible_lines().saturating_sub(1).max(1);
                app.scroll_details_up(page_lines);
            } else {
                let list_height = app.list_height;
                app.move_page_up(list_height);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::PageDown => {
            if app.details_focused() {
                let page_lines = app.details_visible_lines().saturating_sub(1).max(1);
                let total_lines = app.details_total_lines;
                app.scroll_details_down(page_lines, total_lines);
            } else {
                let list_height = app.list_height;
                app.move_page_down(list_height);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Home => {
            if app.details_focused() {
                app.scroll_details_top();
            } else {
                app.jump_to_top();
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::End => {
            if app.details_focused() {
                let total_lines = app.details_total_lines;
                app.scroll_details_bottom(total_lines);
            } else {
                let list_height = app.list_height;
                app.jump_to_bottom(list_height);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('J') if is_plain_char(&key, 'J') => {
            app.execute_palette_command(PaletteCommand::MoveTaskDown, now_rfc3339)
        }
        KeyCode::Enter => app.execute_palette_command(PaletteCommand::RunSelected, now_rfc3339),
        KeyCode::Char('l') if is_plain_char(&key, 'l') => {
            app.execute_palette_command(PaletteCommand::ToggleLoop, now_rfc3339)
        }
        KeyCode::Char('a') if is_plain_char(&key, 'a') => {
            app.execute_palette_command(PaletteCommand::ArchiveTerminal, now_rfc3339)
        }
        KeyCode::Char('d') if is_plain_char(&key, 'd') => {
            if app.selected_task().is_some() {
                app.mode = AppMode::ConfirmDelete;
            } else {
                app.set_status_message("No task selected");
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('e') if is_plain_char(&key, 'e') => {
            if app.selected_task().is_some() {
                app.mode = AppMode::EditingTask {
                    selected: 0,
                    editing_value: None,
                };
            } else {
                app.set_status_message("No task selected");
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('c') if is_plain_char(&key, 'c') => {
            app.mode = AppMode::EditingConfig {
                selected: 0,
                editing_value: None,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('C') if is_plain_char(&key, 'C') => {
            app.execute_palette_command(PaletteCommand::ToggleCaseSensitive, now_rfc3339)
        }
        KeyCode::Char('g') if is_plain_char(&key, 'g') => {
            if app.runner_active {
                app.set_status_message("Runner already active");
            } else {
                app.mode = AppMode::Scanning(TextInput::new(""));
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('G') if is_plain_char(&key, 'G') => {
            app.mode = AppMode::JumpingToTask(TextInput::new(""));
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('n') if is_plain_char(&key, 'n') => {
            app.mode = AppMode::CreatingTask(TextInput::new(""));
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('N') if is_plain_char(&key, 'N') => {
            if app.runner_active {
                app.set_status_message("Runner already active");
            } else {
                app.mode = AppMode::CreatingTaskDescription(TextInput::new(""));
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('/') if is_plain_char(&key, '/') => {
            app.start_search_input();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('t') if is_plain_char(&key, 't') => {
            app.start_filter_tags_input();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('o') if is_plain_char(&key, 'o') => {
            app.start_filter_scopes_input();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('f') if is_plain_char(&key, 'f') => {
            app.cycle_status_filter();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('x') if is_plain_char(&key, 'x') => {
            app.clear_filters();
            app.set_status_message("Filters cleared");
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('s') if is_plain_char(&key, 's') => {
            app.execute_palette_command(PaletteCommand::CycleStatus, now_rfc3339)
        }
        KeyCode::Char('p') if is_plain_char(&key, 'p') => {
            app.execute_palette_command(PaletteCommand::CyclePriority, now_rfc3339)
        }
        KeyCode::Char('r') if is_plain_char(&key, 'r') => {
            app.execute_palette_command(PaletteCommand::ReloadQueue, now_rfc3339)
        }
        KeyCode::Char('R') if is_plain_char(&key, 'R') => {
            app.execute_palette_command(PaletteCommand::ToggleRegex, now_rfc3339)
        }
        KeyCode::Char('v') if is_plain_char(&key, 'v') => {
            app.enter_dependency_graph_mode();
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}
