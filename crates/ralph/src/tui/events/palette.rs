//! Command palette key handling for the TUI.
//!
//! Responsibilities:
//! - Navigate command palette entries and update the query.
//! - Execute the selected command.
//!
//! Not handled here:
//! - Rendering the palette UI.
//! - Definition of palette entries (see `App`).
//!
//! Invariants/assumptions:
//! - Input uses plain characters (Ctrl/Alt modifiers are ignored).

use super::{is_plain_char, text_char, App, AppMode, TuiAction};
use crossterm::event::{KeyCode, KeyEvent};

/// High-level commands available in the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    RunSelected,
    RunNextRunnable,
    ToggleLoop,
    ArchiveTerminal,
    NewTask,
    BuildTaskAgent,
    EditTask,
    EditConfig,
    ScanRepo,
    Search,
    FilterTags,
    FilterScopes,
    ClearFilters,
    CycleStatus,
    CyclePriority,
    ToggleCaseSensitive,
    ToggleRegex,
    ReloadQueue,
    MoveTaskUp,
    MoveTaskDown,
    Quit,
}

/// A single palette entry, already filtered and ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    pub cmd: PaletteCommand,
    pub title: String,
}

/// Handle key events in CommandPalette mode.
pub(super) fn handle_command_palette_key(
    app: &mut App,
    key: KeyEvent,
    query: &str,
    selected: usize,
    now_rfc3339: &str,
) -> anyhow::Result<TuiAction> {
    let entries = app.palette_entries(query);
    let max_index = entries.len().saturating_sub(1);
    let selected = selected.min(max_index);

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Enter => {
            if let Some(entry) = entries.get(selected) {
                app.mode = AppMode::Normal;
                app.execute_palette_command(entry.cmd, now_rfc3339)
            } else {
                app.mode = AppMode::Normal;
                app.set_status_message("No matching command");
                Ok(TuiAction::Continue)
            }
        }
        KeyCode::Up => {
            let next_selected = selected.saturating_sub(1);
            app.mode = AppMode::CommandPalette {
                query: query.to_string(),
                selected: next_selected,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('k') if is_plain_char(&key, 'k') => {
            let next_selected = selected.saturating_sub(1);
            app.mode = AppMode::CommandPalette {
                query: query.to_string(),
                selected: next_selected,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Down => {
            let next_selected = (selected + 1).min(max_index);
            app.mode = AppMode::CommandPalette {
                query: query.to_string(),
                selected: next_selected,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('j') if is_plain_char(&key, 'j') => {
            let next_selected = (selected + 1).min(max_index);
            app.mode = AppMode::CommandPalette {
                query: query.to_string(),
                selected: next_selected,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut next = query.to_string();
            next.pop();
            app.mode = AppMode::CommandPalette {
                query: next,
                selected: 0,
            };
            Ok(TuiAction::Continue)
        }
        _ => {
            if let Some(ch) = text_char(&key) {
                let mut next = query.to_string();
                next.push(ch);
                app.mode = AppMode::CommandPalette {
                    query: next,
                    selected: 0,
                };
            }
            Ok(TuiAction::Continue)
        }
    }
}
