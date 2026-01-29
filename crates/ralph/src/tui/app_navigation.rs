//! Task navigation and selection management for the TUI.
//!
//! Responsibilities:
//! - Task list navigation (up/down/page up/page down).
//! - Jump to task by ID (with case-insensitive matching).
//! - Selection management and scroll position clamping.
//! - Navigation within the filtered task view.
//!
//! Not handled here:
//! - Task mutation operations (see app_tasks module).
//! - Filter management (see app_filters module).
//! - Queue persistence (see app_session module).
//!
//! Invariants/assumptions:
//! - Navigation operates on the filtered view of tasks.
//! - Selection is always clamped to valid range after navigation.
//! - Scroll position follows selection to keep selected task visible.

use crate::contracts::{QueueFile, Task};

/// Trait for task navigation operations.
///
/// This trait provides navigation methods that work with any type
/// that has the required navigation state (selected index, scroll offset,
/// filtered indices, and list height).
pub trait AppNavigation {
    /// Get the currently selected task, if any.
    fn selected_task(&self, queue: &QueueFile) -> Option<&Task>;

    /// Get the currently selected task index in the queue, if any.
    fn selected_task_index(&self) -> Option<usize>;

    /// Get the currently selected task mutably, if any.
    fn selected_task_mut<'a>(&self, queue: &'a mut QueueFile) -> Option<&'a mut Task>;

    /// Set the selected index and clamp to valid range.
    fn set_selected(&mut self, selected: usize);

    /// Move selection up by one.
    fn move_up(&mut self);

    /// Move selection down by one.
    fn move_down(&mut self, list_height: usize);

    /// Move selection up by a page.
    fn move_page_up(&mut self, list_height: usize);

    /// Move selection down by a page.
    fn move_page_down(&mut self, list_height: usize);

    /// Jump selection to the top of the filtered list.
    fn jump_to_top(&mut self);

    /// Jump selection to the bottom of the filtered list.
    fn jump_to_bottom(&mut self, list_height: usize);

    /// Jump to a task by its ID (case-insensitive).
    ///
    /// If the task is found but not visible due to active filters,
    /// filters are cleared first. Returns true if the task was found.
    fn jump_to_task_by_id(
        &mut self,
        id: &str,
        queue: &QueueFile,
        id_to_index: &std::collections::HashMap<String, usize>,
    ) -> bool;

    /// Clamp selection and scroll to valid ranges.
    fn clamp_selection_and_scroll(&mut self);

    /// Get the number of tasks in the filtered view.
    fn filtered_len(&self) -> usize;

    /// Get the current selection index.
    fn selected(&self) -> usize;

    /// Get the current scroll offset.
    fn scroll(&self) -> usize;

    /// Set the scroll offset.
    fn set_scroll(&mut self, scroll: usize);

    /// Get the list height.
    fn list_height(&self) -> usize;

    /// Get a reference to the filtered indices.
    fn filtered_indices(&self) -> &[usize];
}

/// Navigation state that can be composed into larger structs.
#[derive(Debug, Default)]
pub struct NavigationState {
    /// Currently selected task index in the filtered view.
    pub selected: usize,
    /// Scroll offset for the task list.
    pub scroll: usize,
    /// Height of the task list (for scrolling calculation).
    pub list_height: usize,
    /// Cached filtered task indices into the queue.
    pub filtered_indices: Vec<usize>,
}

impl NavigationState {
    /// Create a new navigation state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new navigation state with the given filtered indices.
    pub fn with_filtered_indices(filtered_indices: Vec<usize>) -> Self {
        Self {
            selected: 0,
            scroll: 0,
            list_height: 20,
            filtered_indices,
        }
    }

    /// Get the number of tasks in the filtered view.
    pub fn filtered_len(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Get the currently selected task index in the queue, if any.
    pub fn selected_task_index(&self) -> Option<usize> {
        self.filtered_indices.get(self.selected).copied()
    }

    /// Get the currently selected task, if any.
    pub fn selected_task<'a>(&self, queue: &'a QueueFile) -> Option<&'a Task> {
        self.selected_task_index()
            .and_then(|idx| queue.tasks.get(idx))
    }

    /// Get the currently selected task mutably, if any.
    pub fn selected_task_mut<'a>(&self, queue: &'a mut QueueFile) -> Option<&'a mut Task> {
        self.selected_task_index()
            .and_then(move |idx| queue.tasks.get_mut(idx))
    }

    /// Set the selected index and clamp to valid range.
    pub fn set_selected(&mut self, selected: usize) {
        self.selected = selected;
        self.clamp_selection_and_scroll();
    }

    /// Move selection up by one.
    pub fn move_up(&mut self) {
        if self.filtered_len() > 0 && self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll {
                self.scroll = self.selected;
            }
        }
    }

    /// Move selection down by one.
    pub fn move_down(&mut self, list_height: usize) {
        if self.selected + 1 < self.filtered_len() {
            self.selected += 1;
            if self.selected >= self.scroll + list_height {
                self.scroll = self.selected - list_height + 1;
            }
        }
    }

    /// Move selection up by a page.
    pub fn move_page_up(&mut self, list_height: usize) {
        if self.filtered_len() == 0 {
            return;
        }
        let list_height = list_height.max(1);
        let step = list_height.saturating_sub(1).max(1);
        self.selected = self.selected.saturating_sub(step);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
    }

    /// Move selection down by a page.
    pub fn move_page_down(&mut self, list_height: usize) {
        if self.filtered_len() == 0 {
            return;
        }
        let list_height = list_height.max(1);
        let step = list_height.saturating_sub(1).max(1);
        let max_index = self.filtered_len().saturating_sub(1);
        self.selected = (self.selected + step).min(max_index);
        if self.selected >= self.scroll + list_height {
            self.scroll = self.selected.saturating_sub(list_height.saturating_sub(1));
        }
    }

    /// Jump selection to the top of the filtered list.
    pub fn jump_to_top(&mut self) {
        if self.filtered_len() == 0 {
            self.selected = 0;
            self.scroll = 0;
            return;
        }
        self.selected = 0;
        self.scroll = 0;
    }

    /// Jump selection to the bottom of the filtered list.
    pub fn jump_to_bottom(&mut self, list_height: usize) {
        if self.filtered_len() == 0 {
            self.selected = 0;
            self.scroll = 0;
            return;
        }
        self.selected = self.filtered_len().saturating_sub(1);
        let list_height = list_height.max(1);
        self.scroll = self.selected.saturating_sub(list_height.saturating_sub(1));
    }

    /// Clamp selection and scroll to valid ranges.
    pub fn clamp_selection_and_scroll(&mut self) {
        if self.filtered_indices.is_empty() {
            self.selected = 0;
            self.scroll = 0;
            return;
        }

        if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len().saturating_sub(1);
        }

        if self.scroll > self.selected {
            self.scroll = self.selected;
        }

        let list_height = self.list_height.max(1);
        if self.selected >= self.scroll + list_height {
            self.scroll = self.selected.saturating_sub(list_height.saturating_sub(1));
        }
    }

    /// Find the filtered position of a task by its queue index.
    pub fn position_of(&self, queue_index: usize) -> Option<usize> {
        self.filtered_indices
            .iter()
            .position(|&idx| idx == queue_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Task;

    fn create_task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {}", id),
            ..Default::default()
        }
    }

    fn create_queue_with_tasks(count: usize) -> QueueFile {
        let tasks: Vec<Task> = (0..count)
            .map(|i| create_task(&format!("RQ-{:04}", i)))
            .collect();
        QueueFile {
            tasks,
            ..Default::default()
        }
    }

    #[test]
    fn test_move_up() {
        let _queue = create_queue_with_tasks(5);
        let mut nav = NavigationState::with_filtered_indices(vec![0, 1, 2, 3, 4]);
        nav.selected = 2;
        nav.scroll = 0;

        nav.move_up();
        assert_eq!(nav.selected, 1);
        assert_eq!(nav.scroll, 0);

        nav.move_up();
        assert_eq!(nav.selected, 0);
        assert_eq!(nav.scroll, 0);

        // Can't go below 0
        nav.move_up();
        assert_eq!(nav.selected, 0);
    }

    #[test]
    fn test_move_down() {
        let _queue = create_queue_with_tasks(5);
        let mut nav = NavigationState::with_filtered_indices(vec![0, 1, 2, 3, 4]);
        nav.selected = 0;
        nav.scroll = 0;
        nav.list_height = 3;

        nav.move_down(3);
        assert_eq!(nav.selected, 1);
        assert_eq!(nav.scroll, 0);

        nav.move_down(3);
        assert_eq!(nav.selected, 2);
        assert_eq!(nav.scroll, 0);

        // Should scroll when going past visible area
        nav.move_down(3);
        assert_eq!(nav.selected, 3);
        assert_eq!(nav.scroll, 1);
    }

    #[test]
    fn test_move_page_up() {
        let mut nav = NavigationState::with_filtered_indices(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        nav.selected = 8;
        nav.scroll = 5;

        nav.move_page_up(3);
        assert_eq!(nav.selected, 6); // 8 - (3-1) = 6
        assert_eq!(nav.scroll, 5);

        nav.move_page_up(3);
        assert_eq!(nav.selected, 4);
        assert_eq!(nav.scroll, 4);
    }

    #[test]
    fn test_move_page_down() {
        let mut nav = NavigationState::with_filtered_indices(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        nav.selected = 1;
        nav.scroll = 0;

        nav.move_page_down(3);
        assert_eq!(nav.selected, 3); // 1 + (3-1) = 3
        assert_eq!(nav.scroll, 1);
    }

    #[test]
    fn test_jump_to_top() {
        let mut nav = NavigationState::with_filtered_indices(vec![0, 1, 2, 3, 4]);
        nav.selected = 3;
        nav.scroll = 2;

        nav.jump_to_top();
        assert_eq!(nav.selected, 0);
        assert_eq!(nav.scroll, 0);
    }

    #[test]
    fn test_jump_to_bottom() {
        let mut nav = NavigationState::with_filtered_indices(vec![0, 1, 2, 3, 4]);
        nav.selected = 0;
        nav.scroll = 0;
        nav.list_height = 3;

        nav.jump_to_bottom(3);
        assert_eq!(nav.selected, 4);
        assert_eq!(nav.scroll, 2); // 4 - (3-1) = 2
    }

    #[test]
    fn test_clamp_selection() {
        let mut nav = NavigationState::with_filtered_indices(vec![0, 1, 2]);
        nav.selected = 10; // Out of bounds
        nav.scroll = 5;

        nav.clamp_selection_and_scroll();
        assert_eq!(nav.selected, 2); // Clamped to last valid index
        assert_eq!(nav.scroll, 2); // Scroll also clamped
    }

    #[test]
    fn test_empty_filtered_list() {
        let mut nav = NavigationState::with_filtered_indices(vec![]);
        nav.selected = 0;
        nav.scroll = 0;

        nav.move_up();
        assert_eq!(nav.selected, 0);

        nav.move_down(3);
        assert_eq!(nav.selected, 0);

        nav.jump_to_bottom(3);
        assert_eq!(nav.selected, 0);
        assert_eq!(nav.scroll, 0);
    }

    #[test]
    fn test_position_of() {
        let nav = NavigationState::with_filtered_indices(vec![5, 3, 1, 0, 2]);

        assert_eq!(nav.position_of(3), Some(1));
        assert_eq!(nav.position_of(0), Some(3));
        assert_eq!(nav.position_of(4), None);
    }
}
