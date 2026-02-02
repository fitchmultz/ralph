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

// Implementation for App is in app.rs to avoid circular dependencies
