//! View mode management for the TUI (List vs Board view).
//!
//! Responsibilities:
//! - Switch between List and Board (Kanban) views
//! - Synchronize selection state between views
//! - Update board columns when filters change
//!
//! Not handled here:
//! - Actual board column rendering (see render/panels module)
//! - Board navigation logic (see app_navigation module)
//! - Filter application (see app_filters module)
//!
//! Invariants/assumptions:
//! - View mode switch preserves selected task when possible
//! - Board columns are rebuilt on filter changes
//! - Selection syncing is bidirectional

/// Trait for view mode operations.
pub trait ViewOperations {
    /// Switch to list view.
    ///
    /// Updates the view mode and syncs the list selection to match
    /// the currently selected board task (if any).
    fn switch_to_list_view(&mut self);

    /// Switch to board (Kanban) view.
    ///
    /// Updates the view mode, rebuilds the column task mapping,
    /// and syncs the board selection to match the current list selection.
    fn switch_to_board_view(&mut self);

    /// Sync board navigation selection to list selection.
    ///
    /// Updates the list view's selected index to match the currently
    /// selected task in the board view.
    fn sync_board_selection_to_list(&mut self);

    /// Sync list selection to board navigation.
    ///
    /// Updates the board view's selected column and task to match
    /// the currently selected task in the list view.
    fn sync_list_selection_to_board(&mut self);

    /// Update board column tasks when filters change.
    ///
    /// Should be called after rebuild_filtered_view to keep the board
    /// in sync with the current filter state.
    fn update_board_columns(&mut self);
}

use crate::tui::app::App;
use crate::tui::events::ViewMode;

// Implementation for App
impl ViewOperations for App {
    fn switch_to_list_view(&mut self) {
        if self.view_mode == ViewMode::List {
            return;
        }
        self.view_mode = ViewMode::List;
        self.sync_board_selection_to_list();
        self.set_status_message("Switched to list view (l)");
    }

    fn switch_to_board_view(&mut self) {
        if self.view_mode == ViewMode::Board {
            return;
        }
        self.view_mode = ViewMode::Board;
        self.board_nav
            .update_columns(&self.filtered_indices, &self.queue);
        self.sync_list_selection_to_board();
        self.set_status_message("Switched to board view (b)");
    }

    fn sync_board_selection_to_list(&mut self) {
        if let Some(queue_index) = self.board_nav.selected_task_index()
            && let Some(filtered_pos) = self
                .filtered_indices
                .iter()
                .position(|&idx| idx == queue_index)
        {
            self.selected = filtered_pos;
            self.clamp_selection_and_scroll();
        }
    }

    fn sync_list_selection_to_board(&mut self) {
        if let Some(queue_index) = self.filtered_indices.get(self.selected).copied() {
            self.board_nav.select_task(queue_index, &self.queue);
        }
    }

    fn update_board_columns(&mut self) {
        if self.view_mode == ViewMode::Board {
            self.board_nav
                .update_columns(&self.filtered_indices, &self.queue);
        }
    }
}
