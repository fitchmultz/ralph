//! TUI rendering implementation extracted from `crate::tui`.
//!
//! This module contains all rendering/layout logic for the terminal UI.
//! It is split into focused components (mirroring the `tui/events/` pattern):
//! - `panels`: primary layout panels (task list, task details, execution view)
//! - `overlays`: modal overlays (help, command palette, editors, confirmations)
//! - `footer`: footer help/status line
//! - `utils`: shared text/color helpers
//!
//! Public API is preserved via `crate::tui::draw_ui` re-exporting
//! `render::draw_ui`.

use super::{App, AppMode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

/// Width threshold (in terminal cells) below which the main layout stacks vertically.
const NARROW_LAYOUT_WIDTH: u16 = 90;

mod footer;
mod overlays;
mod panels;
mod utils;

#[cfg(test)]
mod tests;

/// Draw the main UI.
///
/// Public to allow testing with `TestBackend`.
/// Re-exported from `crate::tui` as `tui::draw_ui`.
pub fn draw_ui(f: &mut Frame<'_>, app: &mut App) {
    let size = f.area();

    // Handle Executing mode (full-screen output view), including modal prompts layered on top.
    // Avoid cloning `app.mode` on every frame; we only need to inspect it.
    let show_execution = match &app.mode {
        AppMode::Executing { .. } => true,
        AppMode::ConfirmRevert { previous_mode, .. } => {
            matches!(previous_mode.as_ref(), AppMode::Executing { .. })
        }
        _ => false,
    };

    if show_execution {
        panels::draw_execution_view(f, app, size);
    } else {
        // Reserve a footer row for help + status.
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(2), Constraint::Length(1)].as_ref())
            .split(size);
        let main = outer[0];
        let footer_area = outer[1];

        // Responsive main layout:
        // - If narrow, stack list and details vertically.
        // - If wide, split horizontally.
        let chunks = if main.width < NARROW_LAYOUT_WIDTH {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
                .split(main)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
                .split(main)
        };

        // Left/top panel: task list
        panels::draw_task_list(f, app, chunks[0]);

        // Right/bottom panel: task details
        panels::draw_task_details(f, app, chunks[1]);

        // Footer (help + status).
        footer::draw_footer(f, app, footer_area);
    }

    // Modal overlays layered on top of the base UI.
    match &app.mode {
        AppMode::Help => {
            overlays::draw_help_overlay(f, size);
        }

        // Confirmation dialogs.
        AppMode::ConfirmDelete => {
            overlays::draw_confirm_dialog(f, size, "Delete this task?", "(y/n)");
        }
        AppMode::ConfirmArchive => {
            overlays::draw_confirm_dialog(f, size, "Archive done/rejected tasks?", "(y/n)");
        }
        AppMode::ConfirmQuit => {
            overlays::draw_confirm_dialog(f, size, "Task still running. Quit?", "(y/n)");
        }
        AppMode::ConfirmRevert {
            label,
            allow_proceed,
            selected,
            input,
            ..
        } => {
            overlays::draw_revert_dialog(f, size, label, *allow_proceed, *selected, input);
        }

        // Command palette overlay.
        AppMode::CommandPalette { query, selected } => {
            overlays::draw_command_palette(f, app, size, query, *selected);
        }

        // Config editor overlay.
        AppMode::EditingConfig {
            selected,
            editing_value,
        } => {
            overlays::draw_config_editor(f, app, size, *selected, editing_value.as_deref());
        }

        // Task editor overlay.
        AppMode::EditingTask {
            selected,
            editing_value,
        } => {
            overlays::draw_task_editor(f, app, size, *selected, editing_value.as_deref());
        }

        _ => {}
    }
}
