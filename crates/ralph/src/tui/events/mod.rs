//! TUI event handling extracted from `crate::tui`.
//!
//! Responsibilities:
//! - Dispatch key and mouse events to the active `AppMode` handlers.
//! - Expose `handle_key_event` and shared helpers for input parsing.
//! - Centralize mode-aware keybinding behavior.
//!
//! Not handled here:
//! - Rendering or layout concerns (see `tui::render`).
//! - Queue persistence details or runner execution.
//!
//! Invariants/assumptions:
//! - `AppMode` variants fully describe the active interaction state.
//! - Keybinding behavior remains consistent across handlers.
//! - User-centric shortcuts remain discoverable (e.g. `:` palette, `?`/`h` help).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::foundation::UiEvent;
use super::input::{TextInputEdit, apply_text_input_key};
use super::{App, TextInput};
use crate::tui::app_navigation::NavigationOperations;
use crate::tui::app_panel::PanelOperations;
use crate::tui::app_scroll::ScrollOperations;

pub mod confirm;
pub mod create;
pub mod dependency_graph;
pub mod editing;
pub mod filter;
pub mod flowchart;
pub mod help;
pub mod normal;
pub mod palette;
pub mod run;
pub mod scan;
pub mod search;
pub mod task_builder;
pub mod types;

pub use palette::{PaletteCommand, PaletteEntry, ScoredPaletteEntry};
pub use types::{
    AppMode, ConfirmDiscardAction, TaskBuilderState, TaskBuilderStep, TuiAction, ViewMode,
};

/// Handle a UI event and return the resulting action.
///
/// This is the foundation-aware entry point for event handling. It wraps
/// crossterm events into `UiEvent` and dispatches to appropriate handlers.
/// For now, this delegates to the existing key/mouse handlers while providing
/// a migration path for components using the foundation layer.
#[allow(dead_code)]
pub fn handle_ui_event(
    app: &mut App,
    event: UiEvent,
    now_rfc3339: &str,
) -> anyhow::Result<TuiAction> {
    match event {
        UiEvent::Key(key) => handle_key_event(app, key, now_rfc3339),
        UiEvent::Mouse(mouse) => handle_mouse_event(app, mouse),
        UiEvent::Resize(w, h) => {
            app.set_resized(w, h);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

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
    if should_open_help(app, &key) {
        let previous_mode = app.mode.clone();
        app.enter_help_mode(previous_mode);
        return Ok(TuiAction::Continue);
    }

    match app.mode.clone() {
        AppMode::Normal => normal::handle_normal_mode_key(app, key, now_rfc3339),
        AppMode::Help => help::handle_help_mode_key(app, key),
        AppMode::EditingTask {
            selected,
            editing_value,
        } => editing::handle_editing_task_key(app, key, selected, editing_value, now_rfc3339),
        AppMode::CreatingTask(current) => {
            create::handle_creating_mode_key(app, key, current, now_rfc3339)
        }
        AppMode::CreatingTaskDescription(current) => {
            create::handle_creating_description_mode_key(app, key, current)
        }
        AppMode::Searching(current) => search::handle_searching_mode_key(app, key, current),
        AppMode::FilteringTags(current) => filter::handle_filtering_tags_key(app, key, current),
        AppMode::FilteringScopes(current) => filter::handle_filtering_scopes_key(app, key, current),
        AppMode::EditingConfig {
            selected,
            editing_value,
        } => editing::handle_editing_config_key(app, key, selected, editing_value),
        AppMode::Scanning(current) => scan::handle_scanning_mode_key(app, key, current),
        AppMode::CommandPalette { query, selected } => {
            palette::handle_command_palette_key(app, key, query, selected, now_rfc3339)
        }
        AppMode::ConfirmDelete => confirm::handle_confirm_delete_key(app, key),
        AppMode::ConfirmArchive => confirm::handle_confirm_archive_key(app, key, now_rfc3339),
        AppMode::ConfirmRepair { dry_run } => confirm::handle_confirm_repair_key(app, key, dry_run),
        AppMode::ConfirmUnlock => confirm::handle_confirm_unlock_key(app, key),
        AppMode::ConfirmAutoArchive(task_id) => {
            confirm::handle_confirm_auto_archive_key(app, key, &task_id, now_rfc3339)
        }
        AppMode::ConfirmBatchDelete { count } => {
            confirm::handle_confirm_batch_delete_key(app, key, count)
        }
        AppMode::ConfirmBatchArchive { count } => {
            confirm::handle_confirm_batch_archive_key(app, key, count, now_rfc3339)
        }
        AppMode::ConfirmQuit => confirm::handle_confirm_quit_key(app, key),
        AppMode::ConfirmDiscard { action } => confirm::handle_confirm_discard_key(app, key, action),
        AppMode::ConfirmRevert {
            label,
            preface,
            allow_proceed,
            selected,
            input,
            reply_sender,
            previous_mode,
        } => {
            let state = confirm::ConfirmRevertState::new(
                label,
                preface,
                allow_proceed,
                selected,
                input,
                reply_sender,
                *previous_mode,
            );
            confirm::handle_confirm_revert_key(app, key, state)
        }
        AppMode::ConfirmRiskyConfig {
            key: config_key,
            previous_mode,
            ..
        } => confirm::handle_confirm_risky_config_key(app, key, config_key, *previous_mode),
        AppMode::Executing { .. } => run::handle_executing_mode_key(app, key),
        AppMode::BuildingTaskOptions(state) => {
            task_builder::handle_building_task_options_key(app, key, state)
        }
        AppMode::JumpingToTask(input) => handle_jumping_to_task_key(app, key, input),
        AppMode::FlowchartOverlay { .. } => flowchart::handle_flowchart_mode_key(app, key),
        AppMode::DependencyGraphOverlay { .. } => {
            dependency_graph::handle_dependency_graph_mode_key(app, key)
        }
    }
}

/// Handle a mouse event and return the resulting action.
pub fn handle_mouse_event(app: &mut App, event: MouseEvent) -> anyhow::Result<TuiAction> {
    if app.mode != AppMode::Normal {
        return Ok(TuiAction::Continue);
    }

    match event.kind {
        MouseEventKind::ScrollUp => {
            if app.details_focused() {
                app.scroll_details_up(1);
            } else {
                app.move_up();
            }
        }
        MouseEventKind::ScrollDown => {
            if app.details_focused() {
                app.scroll_details_down(1);
            } else {
                let list_height = app.list_height;
                if list_height > 0 {
                    app.move_down(list_height);
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let Some(area) = app.list_area() else {
                return Ok(TuiAction::Continue);
            };
            if app.filtered_len() == 0 || area.width == 0 || area.height == 0 {
                return Ok(TuiAction::Continue);
            }
            if event.column < area.x
                || event.column >= area.x.saturating_add(area.width)
                || event.row < area.y
                || event.row >= area.y.saturating_add(area.height)
            {
                return Ok(TuiAction::Continue);
            }

            let row_offset = event.row.saturating_sub(area.y) as usize;
            let selected = app.scroll.saturating_add(row_offset);
            if selected < app.filtered_len() {
                app.focus_list_panel();
                app.set_selected(selected);
            }
        }
        _ => {}
    }

    Ok(TuiAction::Continue)
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

pub(super) fn handle_filter_input_key(
    app: &mut App,
    key: KeyEvent,
    mut current: TextInput,
    set_mode: fn(TextInput) -> AppMode,
    apply_value: fn(&mut App, &str),
) -> anyhow::Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            apply_value(app, current.value());
            app.commit_filter_input();
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.restore_filter_snapshot();
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => {
            let before = current.value().to_string();
            if apply_text_input_key(&mut current, &key) == TextInputEdit::Changed {
                if before != current.value() {
                    apply_value(app, current.value());
                }
                app.mode = set_mode(current);
            }
            Ok(TuiAction::Continue)
        }
    }
}

fn should_open_help(app: &App, key: &KeyEvent) -> bool {
    if matches!(app.mode, AppMode::Help | AppMode::FlowchartOverlay { .. }) {
        return false;
    }
    if is_plain_char(key, '?') {
        return true;
    }
    if is_plain_char(key, 'h') && !mode_accepts_text_input(&app.mode) {
        return true;
    }
    false
}

fn mode_accepts_text_input(mode: &AppMode) -> bool {
    match mode {
        AppMode::CreatingTask(_)
        | AppMode::CreatingTaskDescription(_)
        | AppMode::Searching(_)
        | AppMode::FilteringTags(_)
        | AppMode::FilteringScopes(_)
        | AppMode::Scanning(_)
        | AppMode::CommandPalette { .. }
        | AppMode::JumpingToTask(_) => true,
        AppMode::EditingTask { editing_value, .. } => editing_value.is_some(),
        AppMode::EditingConfig { editing_value, .. } => editing_value.is_some(),
        AppMode::ConfirmRevert { selected, .. } => *selected == 2,
        AppMode::BuildingTaskOptions(state) => {
            matches!(
                state.step,
                crate::tui::events::types::TaskBuilderStep::Description
            )
        }
        _ => false,
    }
}

/// Handle key events when in JumpingToTask mode.
fn handle_jumping_to_task_key(
    app: &mut App,
    key: KeyEvent,
    mut input: TextInput,
) -> anyhow::Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            let id = input.value().to_string();
            app.jump_to_task_by_id(&id);
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => {
            if apply_text_input_key(&mut input, &key) == TextInputEdit::Changed {
                app.mode = AppMode::JumpingToTask(input);
            }
            Ok(TuiAction::Continue)
        }
    }
}
