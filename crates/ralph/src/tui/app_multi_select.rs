//! Multi-select mode operations for the TUI.
//!
//! Responsibilities:
//! - Toggle multi-select mode on/off.
//! - Manage the set of selected task indices.
//! - Provide selection queries (count, is_selected).
//!
//! Not handled here:
//! - Batch operations on selections (see app_tasks module).
//! - Visual rendering of selections (see render module).
//! - Navigation while in multi-select mode (see app_navigation module).
//!
//! Invariants/assumptions:
//! - Selection indices refer to positions in filtered_indices, not queue indices.
//! - Exiting multi-select mode clears all selections.
//! - Selections persist across filter changes.

use std::collections::HashSet;

/// Trait for multi-select operations.
///
/// This trait provides methods to manage multi-select mode and selections.
pub trait MultiSelectOperations {
    /// Toggle multi-select mode on/off.
    ///
    /// When exiting multi-select mode, clears all selections.
    fn toggle_multi_select_mode(&mut self);

    /// Toggle selection of the current item.
    ///
    /// Only has effect when in multi-select mode.
    fn toggle_current_selection(&mut self);

    /// Clear all selections and exit multi-select mode.
    fn clear_selection(&mut self);

    /// Get the count of selected items.
    fn selection_count(&self) -> usize;

    /// Check if a filtered position is selected.
    fn is_selected(&self, filtered_idx: usize) -> bool;

    /// Check if multi-select mode is active.
    fn multi_select_mode(&self) -> bool;

    /// Get the selected indices.
    fn selected_indices(&self) -> &HashSet<usize>;

    /// Add an index to the selection.
    fn select_index(&mut self, idx: usize);

    /// Remove an index from the selection.
    fn deselect_index(&mut self, idx: usize);
}

/// State for multi-select operations.
#[derive(Debug, Default)]
pub struct MultiSelectState {
    /// Multi-select mode flag - when true, navigation keeps selections.
    pub multi_select_mode: bool,
    /// Set of selected task indices (positions in filtered_indices, not queue indices).
    pub selected_indices: HashSet<usize>,
}

impl MultiSelectState {
    /// Create a new multi-select state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle multi-select mode on/off.
    pub fn toggle_mode(&mut self) {
        self.multi_select_mode = !self.multi_select_mode;
        if !self.multi_select_mode {
            self.selected_indices.clear();
        }
    }

    /// Toggle selection of the current index.
    pub fn toggle_selection(&mut self, current_idx: usize) {
        if !self.multi_select_mode {
            return;
        }
        if self.selected_indices.contains(&current_idx) {
            self.selected_indices.remove(&current_idx);
        } else {
            self.selected_indices.insert(current_idx);
        }
    }

    /// Clear all selections and exit multi-select mode.
    pub fn clear(&mut self) {
        self.selected_indices.clear();
        self.multi_select_mode = false;
    }

    /// Get the count of selected items.
    pub fn count(&self) -> usize {
        self.selected_indices.len()
    }

    /// Check if an index is selected.
    pub fn is_selected(&self, idx: usize) -> bool {
        self.selected_indices.contains(&idx)
    }

    /// Get the selected indices as a slice (returns empty slice if none).
    pub fn indices(&self) -> Vec<usize> {
        self.selected_indices.iter().copied().collect()
    }
}

use crate::tui::App;

impl MultiSelectOperations for App {
    fn toggle_multi_select_mode(&mut self) {
        self.multi_select_mode = !self.multi_select_mode;
        if !self.multi_select_mode {
            self.selected_indices.clear();
        }
    }

    fn toggle_current_selection(&mut self) {
        if !self.multi_select_mode {
            return;
        }
        if self.selected_indices.contains(&self.selected) {
            self.selected_indices.remove(&self.selected);
        } else {
            self.selected_indices.insert(self.selected);
        }
    }

    fn clear_selection(&mut self) {
        self.selected_indices.clear();
        self.multi_select_mode = false;
    }

    fn selection_count(&self) -> usize {
        self.selected_indices.len()
    }

    fn is_selected(&self, filtered_idx: usize) -> bool {
        self.selected_indices.contains(&filtered_idx)
    }

    fn multi_select_mode(&self) -> bool {
        self.multi_select_mode
    }

    fn selected_indices(&self) -> &HashSet<usize> {
        &self.selected_indices
    }

    fn select_index(&mut self, idx: usize) {
        self.selected_indices.insert(idx);
    }

    fn deselect_index(&mut self, idx: usize) {
        self.selected_indices.remove(&idx);
    }
}
