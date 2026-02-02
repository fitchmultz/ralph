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
//! - Input uses cursor-aware `TextInput` edits.

use super::super::TextInput;
use super::super::input::{TextInputEdit, apply_text_input_key};
use super::{App, AppMode, TuiAction};
use crate::tui::app_palette_ops::PaletteOperations;
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
    SetStatusDraft,
    SetStatusTodo,
    SetStatusDoing,
    SetStatusDone,
    SetStatusRejected,
    SetPriorityCritical,
    SetPriorityHigh,
    SetPriorityMedium,
    SetPriorityLow,
    ToggleCaseSensitive,
    ToggleRegex,
    ToggleFuzzy,
    ReloadQueue,
    MoveTaskUp,
    MoveTaskDown,
    JumpToTask,
    RepairQueue,
    RepairQueueDryRun,
    UnlockQueue,
    /// Toggle multi-select mode
    ToggleMultiSelectMode,
    /// Toggle selection of current task (Space in multi-select mode)
    ToggleTaskSelection,
    /// Batch delete selected tasks
    BatchDelete,
    /// Batch archive selected tasks
    BatchArchive,
    /// Batch set status on selected tasks
    BatchSetStatus(crate::contracts::TaskStatus),
    /// Clear all selections
    ClearSelection,
    Quit,
}

/// A single palette entry, already filtered and ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    pub cmd: PaletteCommand,
    pub title: String,
}

/// A scored palette entry for ranking search results.
///
/// Used internally by the fuzzy matching algorithm to sort palette entries
/// by relevance score while preserving stability through original_index.
#[derive(Debug, Clone)]
pub struct ScoredPaletteEntry {
    pub entry: PaletteEntry,
    pub score: i32,
    pub original_index: usize,
}

/// Handle key events in CommandPalette mode.
pub(super) fn handle_command_palette_key(
    app: &mut App,
    key: KeyEvent,
    mut query: TextInput,
    selected: usize,
    now_rfc3339: &str,
) -> anyhow::Result<TuiAction> {
    let entries = app.palette_entries(query.value());
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
                query,
                selected: next_selected,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Down => {
            let next_selected = (selected + 1).min(max_index);
            app.mode = AppMode::CommandPalette {
                query,
                selected: next_selected,
            };
            Ok(TuiAction::Continue)
        }
        _ => {
            let before = query.value().to_string();
            if apply_text_input_key(&mut query, &key) == TextInputEdit::Changed {
                let value_changed = before != query.value();
                app.mode = AppMode::CommandPalette {
                    query,
                    selected: if value_changed { 0 } else { selected },
                };
            }
            Ok(TuiAction::Continue)
        }
    }
}
