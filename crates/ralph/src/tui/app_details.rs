//! Details panel state management for the TUI.
//!
//! Responsibilities:
//! - Track details panel scroll position and viewport state.
//! - Manage details context for detecting content changes.
//! - Provide scrolling methods (up, down, top, bottom).
//!
//! Not handled here:
//! - Details content rendering (handled by render module).
//! - Task detail editing (handled by task_edit module).
//! - Panel focus management (handled by app module).
//!
//! Invariants/assumptions:
//! - Scroll positions are clamped to valid ranges based on visible/total lines.
//! - Context changes trigger scroll reset to top.
//! - `visible_lines` and `total_lines` are updated by the renderer after layout.

use crate::tui::events::AppMode;

/// Context key for details content (used to reset scroll on change).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailsContextMode {
    /// Showing task details.
    TaskDetails,
    /// Creating a new task.
    CreatingTask,
    /// Creating a new task with description.
    CreatingTaskDescription,
    /// Searching tasks.
    Searching,
    /// Filtering by tags.
    FilteringTags,
    /// Scanning repository.
    Scanning,
    /// Empty queue state.
    EmptyQueue,
    /// Filtered view is empty.
    FilteredEmpty { summary: String },
}

/// Context for the details panel to detect content changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsContext {
    /// Current mode/context of the details panel.
    pub mode: DetailsContextMode,
    /// ID of the selected task, if any.
    pub selected_id: Option<String>,
    /// Queue revision when context was set.
    pub queue_rev: u64,
    /// Width of the detail panel for text wrapping.
    pub detail_width: u16,
}

/// State for the details panel.
#[derive(Debug)]
pub struct DetailsState {
    /// Scroll offset for the details panel.
    scroll: usize,
    /// Last known visible detail lines (for paging).
    visible_lines: usize,
    /// Last known total detail line count (post-wrap).
    total_lines: usize,
    /// Context key for details content (used to reset scroll on change).
    context: Option<DetailsContext>,
}

impl Default for DetailsState {
    fn default() -> Self {
        Self {
            scroll: 0,
            visible_lines: 1,
            total_lines: 0,
            context: None,
        }
    }
}

impl DetailsState {
    /// Create a new details state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the current scroll position.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Get the number of visible lines.
    pub fn visible_lines(&self) -> usize {
        self.visible_lines.max(1)
    }

    /// Get the total number of lines.
    pub fn total_lines(&self) -> usize {
        self.total_lines
    }

    /// Get the current context, if any.
    pub fn context(&self) -> Option<&DetailsContext> {
        self.context.as_ref()
    }

    /// Calculate the maximum valid scroll position.
    pub fn max_scroll(&self, total_lines: usize) -> usize {
        total_lines.saturating_sub(self.visible_lines())
    }

    /// Update the viewport and context, resetting scroll if context changed.
    ///
    /// If the context has changed, scroll is reset to 0. Otherwise,
    /// scroll is clamped to the valid range for the new viewport.
    pub fn set_viewport(
        &mut self,
        visible_lines: usize,
        total_lines: usize,
        context: DetailsContext,
    ) {
        let visible_lines = visible_lines.max(1);

        // Reset scroll if context changed
        if self.context.as_ref() != Some(&context) {
            self.scroll = 0;
            self.context = Some(context);
        }

        self.visible_lines = visible_lines;
        self.total_lines = total_lines;

        // Clamp scroll to valid range
        let max_scroll = self.max_scroll(total_lines);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// Scroll up by the specified number of lines.
    pub fn scroll_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        self.scroll = self.scroll.saturating_sub(lines);
    }

    /// Scroll down by the specified number of lines.
    pub fn scroll_down(&mut self, lines: usize, total_lines: usize) {
        if lines == 0 {
            return;
        }
        let max_scroll = self.max_scroll(total_lines);
        self.scroll = (self.scroll + lines).min(max_scroll);
    }

    /// Scroll to the top.
    pub fn scroll_top(&mut self) {
        self.scroll = 0;
    }

    /// Scroll to the bottom.
    pub fn scroll_bottom(&mut self, total_lines: usize) {
        self.scroll = self.max_scroll(total_lines);
    }

    /// Reset the state.
    pub fn reset(&mut self) {
        self.scroll = 0;
        self.visible_lines = 1;
        self.total_lines = 0;
        self.context = None;
    }

    /// Check if the context matches the given parameters.
    pub fn context_matches(&self, mode: &DetailsContextMode, selected_id: Option<&str>) -> bool {
        self.context
            .as_ref()
            .is_some_and(|ctx| ctx.mode == *mode && ctx.selected_id.as_deref() == selected_id)
    }

    /// Update context from current app state.
    ///
    /// This is a convenience method to create a context from common app state.
    pub fn update_from_app_state(
        &mut self,
        mode: AppMode,
        selected_id: Option<String>,
        queue_rev: u64,
        detail_width: u16,
        visible_lines: usize,
        total_lines: usize,
    ) {
        let context_mode = match mode {
            AppMode::CreatingTask(_) => DetailsContextMode::CreatingTask,
            AppMode::CreatingTaskDescription(_) => DetailsContextMode::CreatingTaskDescription,
            AppMode::Searching(_) => DetailsContextMode::Searching,
            AppMode::FilteringTags(_) => DetailsContextMode::FilteringTags,
            AppMode::FilteringScopes(_) => DetailsContextMode::FilteringTags,
            AppMode::Scanning(_) => DetailsContextMode::Scanning,
            _ => DetailsContextMode::TaskDetails,
        };

        self.set_viewport(
            visible_lines,
            total_lines,
            DetailsContext {
                mode: context_mode,
                selected_id,
                queue_rev,
                detail_width,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_clamping() {
        let mut state = DetailsState::new();
        state.set_viewport(
            10,
            100,
            DetailsContext {
                mode: DetailsContextMode::TaskDetails,
                selected_id: Some("RQ-0001".to_string()),
                queue_rev: 1,
                detail_width: 60,
            },
        );

        // Scroll down should be clamped
        state.scroll_down(1000, 100);
        assert_eq!(state.scroll(), 90); // 100 - 10

        // Scroll up
        state.scroll_up(50);
        assert_eq!(state.scroll(), 40);

        // Scroll top
        state.scroll_top();
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn test_context_change_resets_scroll() {
        let mut state = DetailsState::new();
        state.set_viewport(
            10,
            100,
            DetailsContext {
                mode: DetailsContextMode::TaskDetails,
                selected_id: Some("RQ-0001".to_string()),
                queue_rev: 1,
                detail_width: 60,
            },
        );

        // Scroll down
        state.scroll_down(50, 100);
        assert_eq!(state.scroll(), 50);

        // Change context - scroll should reset
        state.set_viewport(
            10,
            100,
            DetailsContext {
                mode: DetailsContextMode::TaskDetails,
                selected_id: Some("RQ-0002".to_string()), // Different task
                queue_rev: 1,
                detail_width: 60,
            },
        );
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn test_same_context_preserves_scroll() {
        let mut state = DetailsState::new();
        let context = DetailsContext {
            mode: DetailsContextMode::TaskDetails,
            selected_id: Some("RQ-0001".to_string()),
            queue_rev: 1,
            detail_width: 60,
        };

        state.set_viewport(10, 100, context.clone());
        state.scroll_down(30, 100);
        assert_eq!(state.scroll(), 30);

        // Same context - scroll should be preserved (but clamped if needed)
        state.set_viewport(10, 100, context);
        assert_eq!(state.scroll(), 30);
    }

    #[test]
    fn test_viewport_update_clamps_scroll() {
        let mut state = DetailsState::new();
        state.set_viewport(
            20,
            100,
            DetailsContext {
                mode: DetailsContextMode::TaskDetails,
                selected_id: Some("RQ-0001".to_string()),
                queue_rev: 1,
                detail_width: 60,
            },
        );

        state.scroll_down(80, 100); // Scroll to 80 (max with 20 visible is 80)
        assert_eq!(state.scroll(), 80);

        // Reduce visible lines significantly - scroll should be clamped
        state.set_viewport(
            5,
            100,
            DetailsContext {
                mode: DetailsContextMode::TaskDetails,
                selected_id: Some("RQ-0001".to_string()),
                queue_rev: 1,
                detail_width: 60,
            },
        );
        // scroll is 80, new max is 95 (100 - 5), 80 <= 95 so no clamping
        assert_eq!(state.scroll(), 80);

        // Scroll to new max
        state.scroll_down(20, 100);
        assert_eq!(state.scroll(), 95); // Clamped to new max (100 - 5 = 95)
    }

    #[test]
    fn test_context_matches() {
        let mut state = DetailsState::new();
        state.set_viewport(
            10,
            100,
            DetailsContext {
                mode: DetailsContextMode::TaskDetails,
                selected_id: Some("RQ-0001".to_string()),
                queue_rev: 1,
                detail_width: 60,
            },
        );

        assert!(state.context_matches(&DetailsContextMode::TaskDetails, Some("RQ-0001")));
        assert!(!state.context_matches(&DetailsContextMode::TaskDetails, Some("RQ-0002")));
        assert!(!state.context_matches(&DetailsContextMode::CreatingTask, Some("RQ-0001")));
    }

    #[test]
    fn test_reset() {
        let mut state = DetailsState::new();
        state.set_viewport(
            10,
            100,
            DetailsContext {
                mode: DetailsContextMode::TaskDetails,
                selected_id: Some("RQ-0001".to_string()),
                queue_rev: 1,
                detail_width: 60,
            },
        );
        state.scroll_down(50, 100);

        state.reset();

        assert_eq!(state.scroll(), 0);
        assert_eq!(state.visible_lines(), 1);
        assert_eq!(state.total_lines(), 0);
        assert!(state.context().is_none());
    }
}
