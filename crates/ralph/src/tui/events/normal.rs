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
use super::types::{TuiAction, ViewMode};
use super::{App, is_ctrl_char, is_plain_char};
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
                app.scroll_details_down(1);
            } else {
                let list_height = app.list_height;
                app.move_down(list_height);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('j') if is_plain_char(&key, 'j') => {
            if app.details_focused() {
                app.scroll_details_down(1);
            } else if app.view_mode == ViewMode::Board {
                app.board_nav.move_down();
                app.sync_board_selection_to_list();
            } else {
                let list_height = app.list_height;
                app.move_down(list_height);
            }
            Ok(TuiAction::Continue)
        }
        // Board view navigation: Left/Right for columns
        KeyCode::Left => {
            if app.view_mode == ViewMode::Board {
                app.board_nav.move_left();
                app.sync_board_selection_to_list();
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('h') if is_plain_char(&key, 'h') => {
            if app.details_focused() {
                app.scroll_details_up(1);
            } else {
                app.move_up();
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Right => {
            if app.view_mode == ViewMode::Board {
                app.board_nav.move_right();
                app.sync_board_selection_to_list();
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::PageUp => {
            if app.details_focused() {
                // Use a reasonable default for page scroll (viewport height - 1)
                let page_lines = 10;
                app.scroll_details_up(page_lines);
            } else {
                let list_height = app.list_height;
                app.move_page_up(list_height);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::PageDown => {
            if app.details_focused() {
                let page_lines = 10;
                app.scroll_details_down(page_lines);
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
                app.scroll_details_bottom();
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
        // View mode toggles: 'l' for list view, 'b' for board view
        KeyCode::Char('l') if is_plain_char(&key, 'l') => {
            app.switch_to_list_view();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('b') if is_plain_char(&key, 'b') => {
            app.switch_to_board_view();
            Ok(TuiAction::Continue)
        }
        // Loop toggle moved to 'L' (capital) to free up 'l' for list view
        KeyCode::Char('L') if is_plain_char(&key, 'L') => {
            app.execute_palette_command(PaletteCommand::ToggleLoop, now_rfc3339)
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
        KeyCode::Char('f') if is_ctrl_char(&key, 'f') => {
            app.execute_palette_command(PaletteCommand::ToggleFuzzy, now_rfc3339)
        }
        KeyCode::Char('v') if is_plain_char(&key, 'v') => {
            app.enter_dependency_graph_mode();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(' ') if is_plain_char(&key, ' ') => {
            if app.multi_select_mode {
                app.toggle_current_selection();
                let count = app.selection_count();
                app.set_status_message(format!("{} tasks selected", count));
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('m') if is_plain_char(&key, 'm') => {
            app.toggle_multi_select_mode();
            if app.multi_select_mode {
                app.set_status_message(
                    "Multi-select mode ON. Space: toggle, m: exit, a: archive, d: delete"
                        .to_string(),
                );
            } else {
                app.set_status_message("Multi-select mode OFF".to_string());
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('d') if is_plain_char(&key, 'd') => {
            if app.multi_select_mode && !app.selected_indices.is_empty() {
                app.mode = AppMode::ConfirmBatchDelete {
                    count: app.selection_count(),
                };
            } else if app.selected_task().is_some() {
                app.mode = AppMode::ConfirmDelete;
            } else {
                app.set_status_message("No task selected");
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('a') if is_plain_char(&key, 'a') => {
            if app.multi_select_mode && !app.selected_indices.is_empty() {
                app.mode = AppMode::ConfirmBatchArchive {
                    count: app.selection_count(),
                };
            } else {
                app.execute_palette_command(PaletteCommand::ArchiveTerminal, now_rfc3339)?;
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            if app.multi_select_mode {
                app.clear_selection();
                app.set_status_message("Selection cleared".to_string());
            } else {
                return app.execute_palette_command(PaletteCommand::Quit, now_rfc3339);
            }
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}
