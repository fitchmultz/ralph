//! TUI rendering implementation extracted from `crate::tui`.
//!
//! Responsibilities:
//! - Render the terminal UI layout and mode-specific overlays.
//! - Provide shared render helpers for panels, footer, and overlay components.
//! - Expose the `draw_ui` entrypoint re-exported by `crate::tui`.
//!
//! Not handled here:
//! - Event handling or key dispatch (see `tui::events`).
//! - Queue mutations or runner execution side effects.
//!
//! Invariants/assumptions:
//! - Rendering operates on the current `App` state; some overlays may update
//!   internal caches (e.g., dependency graph cache) for performance.
//! - Layout components remain consistent with TUI styling conventions.

use crate::constants::ui::NARROW_LAYOUT_WIDTH;

use super::events::types::{ConfirmDiscardAction, ViewMode};
use super::{App, AppMode};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

mod board;
mod footer;
mod header;
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

    // Handle resize flag: clear it and ensure fresh layout calculations
    if app.take_resized() {
        // Force recalculation of layout-dependent state
        app.detail_width = size.width.saturating_sub(4);
    }

    // Handle Executing mode (full-screen output view), including modal prompts layered on top.
    // Avoid cloning `app.mode` on every frame; we only need to inspect it.
    let show_execution = match &app.mode {
        AppMode::Executing { .. } => true,
        AppMode::ConfirmRevert { previous_mode, .. } => {
            matches!(previous_mode.as_ref(), AppMode::Executing { .. })
        }
        AppMode::Help => matches!(app.help_previous_mode(), Some(AppMode::Executing { .. })),
        _ => false,
    };

    if show_execution {
        panels::draw_execution_view(f, app, size);
    } else {
        // Three-row layout: header, main content, footer
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(1), // header
                    Constraint::Min(2),    // main content
                    Constraint::Length(1), // footer
                ]
                .as_ref(),
            )
            .split(size);
        let header_area = outer[0];
        let main = outer[1];
        let footer_area = outer[2];

        // Draw header (mode, dirty state, filters, runner status)
        header::draw_header(f, app, header_area);

        // Branch based on view mode: list or board
        match app.view_mode {
            ViewMode::List => {
                // Responsive main layout:
                // - If narrow, stack list and details vertically.
                // - If wide, split horizontally.
                let chunks = if main.width < NARROW_LAYOUT_WIDTH {
                    Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [Constraint::Percentage(45), Constraint::Percentage(55)].as_ref(),
                        )
                        .split(main)
                } else {
                    Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(
                            [Constraint::Percentage(45), Constraint::Percentage(55)].as_ref(),
                        )
                        .split(main)
                };

                // Left/top panel: task list
                panels::draw_task_list(f, app, chunks[0]);

                // Right/bottom panel: task details
                panels::draw_task_details(f, app, chunks[1]);
            }
            ViewMode::Board => {
                // Board view: Kanban columns with optional details panel
                // If wide enough, show board + details side by side
                // If narrow, show only board
                if main.width >= 140 {
                    let chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(
                            [Constraint::Percentage(70), Constraint::Percentage(30)].as_ref(),
                        )
                        .split(main);

                    // Left panel: Kanban board
                    board::draw_kanban_board(f, app, chunks[0]);

                    // Right panel: task details
                    panels::draw_task_details(f, app, chunks[1]);
                } else {
                    // Full-width board view
                    board::draw_kanban_board(f, app, main);
                }
            }
        }

        // Footer (help + status).
        footer::draw_footer(f, app, footer_area);
    }

    // Modal overlays layered on top of the base UI.
    match &app.mode {
        AppMode::Help => {
            overlays::draw_help_overlay(f, app, size);
        }

        // Confirmation dialogs.
        AppMode::ConfirmDelete => {
            overlays::draw_confirm_dialog(f, size, "Delete this task?", "(y/n)");
        }
        AppMode::ConfirmArchive => {
            overlays::draw_confirm_dialog(f, size, "Archive done/rejected tasks?", "(y/n)");
        }
        AppMode::ConfirmRepair { dry_run: true } => {
            overlays::draw_confirm_dialog(f, size, "Repair queue (dry run)?", "(y/n)");
        }
        AppMode::ConfirmRepair { dry_run: false } => {
            overlays::draw_confirm_dialog(
                f,
                size,
                "Repair queue? This will modify files.",
                "(y/n)",
            );
        }
        AppMode::ConfirmUnlock => {
            overlays::draw_confirm_dialog(
                f,
                size,
                "Unlock queue? This removes the lock file.",
                "(y/n)",
            );
        }
        AppMode::ConfirmAutoArchive(task_id) => {
            let message = format!("Archive task {}?", task_id);
            overlays::draw_confirm_dialog(f, size, &message, "(y/n)");
        }
        AppMode::ConfirmQuit => {
            overlays::draw_confirm_dialog(f, size, "Task still running. Quit?", "(y/n)");
        }
        AppMode::ConfirmDiscard { action } => {
            let message = match action {
                ConfirmDiscardAction::ReloadQueue => "Reload and discard unsaved changes?",
                ConfirmDiscardAction::Quit => "Quit and discard unsaved changes?",
            };
            overlays::draw_confirm_dialog(f, size, message, "(y/n)");
        }
        AppMode::ConfirmRevert {
            label,
            preface,
            allow_proceed,
            selected,
            input,
            ..
        } => {
            overlays::draw_revert_dialog(
                f,
                size,
                label,
                preface.as_deref(),
                *allow_proceed,
                *selected,
                input,
            );
        }
        AppMode::ConfirmRiskyConfig { warning, .. } => {
            overlays::draw_risky_config_dialog(f, size, warning);
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
            overlays::draw_config_editor(f, app, size, *selected, editing_value.as_ref());
        }

        // Task editor overlay.
        AppMode::EditingTask {
            selected,
            editing_value,
        } => {
            overlays::draw_task_editor(f, app, size, *selected, editing_value.as_ref());
        }

        // Task builder overlay.
        AppMode::BuildingTaskOptions(state) => {
            overlays::draw_task_builder(f, size, state);
        }

        // Jump to task by ID overlay.
        AppMode::JumpingToTask(input) => {
            overlays::draw_jump_to_task_input(f, size, input);
        }

        // Workflow flowchart overlay.
        AppMode::FlowchartOverlay { .. } => {
            overlays::draw_flowchart_overlay(f, app, size);
        }

        // Dependency graph overlay.
        AppMode::DependencyGraphOverlay {
            show_dependents,
            highlight_critical,
            ..
        } => {
            overlays::draw_dependency_graph_overlay(
                f,
                app,
                size,
                *show_dependents,
                *highlight_critical,
            );
        }

        _ => {}
    }
}
