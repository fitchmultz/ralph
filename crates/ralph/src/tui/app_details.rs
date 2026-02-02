//! Details panel state management for the TUI.
//!
//! Responsibilities:
//! - Track details panel scroll position using tui-scrollview's ScrollViewState.
//! - Manage details context for detecting content changes.
//! - Provide scrolling methods (up, down, top, bottom).
//!
//! Not handled here:
//! - Details content rendering (handled by render module).
//! - Task detail editing (handled by task_edit module).
//! - Panel focus management (handled by app module).
//! - Scroll clamping (handled by ScrollViewState).
//!
//! Invariants/assumptions:
//! - Context changes trigger scroll reset to top.
//! - ScrollViewState manages internal scroll offset and bounds.

use crate::tui::events::AppMode;
use tui_scrollview::ScrollViewState;

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
#[derive(Debug, Default)]
pub struct DetailsState {
    /// ScrollView state for smooth scrolling.
    scroll_state: ScrollViewState,
    /// Context key for details content (used to reset scroll on change).
    context: Option<DetailsContext>,
}

impl DetailsState {
    /// Create a new details state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the current scroll position.
    pub fn scroll(&self) -> usize {
        self.scroll_state.offset().y as usize
    }

    /// Get the ScrollViewState for rendering.
    pub fn scroll_state(&mut self) -> &mut ScrollViewState {
        &mut self.scroll_state
    }

    /// Get the current context, if any.
    pub fn context(&self) -> Option<&DetailsContext> {
        self.context.as_ref()
    }

    /// Update the viewport and context, resetting scroll if context changed.
    ///
    /// If the context has changed, scroll is reset to top.
    /// If the context is the same, scroll is clamped to ensure it stays within
    /// visible bounds (called on resize to prevent scroll from exceeding content).
    pub fn set_viewport(
        &mut self,
        visible_lines: usize,
        total_lines: usize,
        context: DetailsContext,
    ) {
        // Reset scroll if context changed
        if self.context.as_ref() != Some(&context) {
            self.scroll_state.scroll_to_top();
            self.context = Some(context);
        } else {
            // Same context - clamp scroll to ensure it stays within bounds
            // This handles terminal resize where visible_lines may have changed
            self.clamp_scroll(visible_lines, total_lines);
        }
    }

    /// Scroll up by the specified number of lines.
    pub fn scroll_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        // tui-scrollview 0.5 scroll_up/down don't take arguments, scroll by 1 line
        // We use set_offset for multi-line scrolling
        let current = self.scroll_state.offset();
        let new_y = current.y.saturating_sub(lines as u16);
        self.scroll_state
            .set_offset(ratatui::layout::Position::new(current.x, new_y));
    }

    /// Scroll down by the specified number of lines.
    pub fn scroll_down(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        // tui-scrollview 0.5 scroll_up/down don't take arguments, scroll by 1 line
        // We use set_offset for multi-line scrolling
        let current = self.scroll_state.offset();
        let new_y = current.y.saturating_add(lines as u16);
        self.scroll_state
            .set_offset(ratatui::layout::Position::new(current.x, new_y));
    }

    /// Scroll to the top.
    pub fn scroll_top(&mut self) {
        self.scroll_state.scroll_to_top();
    }

    /// Scroll to the bottom.
    pub fn scroll_bottom(&mut self) {
        self.scroll_state.scroll_to_bottom();
    }

    /// Reset the state.
    pub fn reset(&mut self) {
        self.scroll_state = ScrollViewState::default();
        self.context = None;
    }

    /// Check if the context matches the given parameters.
    pub fn context_matches(&self, mode: &DetailsContextMode, selected_id: Option<&str>) -> bool {
        self.context
            .as_ref()
            .is_some_and(|ctx| ctx.mode == *mode && ctx.selected_id.as_deref() == selected_id)
    }

    /// Clamp scroll position to ensure it stays within visible bounds.
    ///
    /// This should be called after a terminal resize to prevent the scroll
    /// position from being outside the visible content area.
    pub fn clamp_scroll(&mut self, visible_lines: usize, total_lines: usize) {
        if total_lines == 0 || visible_lines == 0 {
            self.scroll_state.scroll_to_top();
            return;
        }

        let max_scroll = total_lines.saturating_sub(visible_lines);
        let current = self.scroll_state.offset().y as usize;

        if current > max_scroll {
            let new_y = max_scroll.min(u16::MAX as usize) as u16;
            self.scroll_state
                .set_offset(ratatui::layout::Position::new(0, new_y));
        }
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
    fn test_scroll_operations() {
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

        // Initial scroll should be 0
        assert_eq!(state.scroll(), 0);

        // Scroll down
        state.scroll_down(50);
        assert_eq!(state.scroll(), 50);

        // Scroll up
        state.scroll_up(20);
        assert_eq!(state.scroll(), 30);

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
        state.scroll_down(50);
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
        state.scroll_down(30);
        assert_eq!(state.scroll(), 30);

        // Same context - scroll should be preserved
        state.set_viewport(10, 100, context);
        assert_eq!(state.scroll(), 30);
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
        state.scroll_down(50);
        assert_eq!(state.scroll(), 50);

        state.reset();

        assert_eq!(state.scroll(), 0);
        assert!(state.context().is_none());
    }

    #[test]
    fn test_scroll_state_accessor() {
        let mut state = DetailsState::new();
        // Verify we can get mutable access to scroll state for rendering
        let scroll_state = state.scroll_state();
        scroll_state.scroll_down();

        assert_eq!(state.scroll(), 1);
    }

    #[test]
    fn test_clamp_scroll_when_beyond_bounds() {
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

        // Scroll down significantly
        state.scroll_down(80);
        assert_eq!(state.scroll(), 80);

        // Simulate resize that reduces visible lines, clamping scroll
        state.clamp_scroll(10, 50); // Now only 50 total lines, 10 visible

        // Scroll should be clamped to max_scroll (50 - 10 = 40)
        assert_eq!(state.scroll(), 40);
    }

    #[test]
    fn test_clamp_scroll_when_within_bounds() {
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

        // Scroll to a reasonable position
        state.scroll_down(20);
        assert_eq!(state.scroll(), 20);

        // Clamp with larger bounds - should not change
        state.clamp_scroll(10, 100);

        // Scroll should remain unchanged
        assert_eq!(state.scroll(), 20);
    }

    #[test]
    fn test_clamp_scroll_empty_content() {
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
        state.scroll_down(50);
        assert_eq!(state.scroll(), 50);

        // Clamp with zero total lines - should reset to top
        state.clamp_scroll(10, 0);

        // Scroll should be reset to 0
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn test_clamp_scroll_zero_visible() {
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
        state.scroll_down(50);
        assert_eq!(state.scroll(), 50);

        // Clamp with zero visible lines - should reset to top
        state.clamp_scroll(0, 100);

        // Scroll should be reset to 0
        assert_eq!(state.scroll(), 0);
    }
}
