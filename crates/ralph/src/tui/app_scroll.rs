//! Scroll management for details, help, and log panels.
//!
//! Responsibilities:
//! - Manage scroll position for details panel (using tui-scrollview)
//! - Manage scroll position for help overlay
//! - Coordinate log scroll with auto-scroll behavior
//!
//! Not handled here:
//! - Actual content rendering (see render module)
//! - Scrollbar drawing (handled by ratatui components)
//! - Content line counting (done at render time)
//!
//! Invariants/assumptions:
//! - Scroll positions are clamped to valid ranges
//! - Auto-scroll keeps logs at bottom when enabled
//! - Context changes reset details scroll position

use crate::tui::app_details::DetailsContext;

/// Trait for scroll operations on the TUI application.
pub trait ScrollOperations {
    /// Scroll the details panel up by the specified number of lines.
    fn scroll_details_up(&mut self, lines: usize);

    /// Scroll the details panel down by the specified number of lines.
    fn scroll_details_down(&mut self, lines: usize);

    /// Scroll the details panel to the top.
    fn scroll_details_top(&mut self);

    /// Scroll the details panel to the bottom.
    fn scroll_details_bottom(&mut self);

    /// Get the current details scroll position.
    fn details_scroll(&self) -> usize;

    /// Get mutable access to the details scroll state for rendering.
    fn details_scroll_state(&mut self) -> &mut tui_scrollview::ScrollViewState;

    /// Update details viewport information and reset scroll if context changed.
    fn set_details_viewport(
        &mut self,
        visible_lines: usize,
        total_lines: usize,
        context: DetailsContext,
    );

    /// Scroll the help panel up by the specified number of lines.
    fn scroll_help_up(&mut self, lines: usize);

    /// Scroll the help panel down by the specified number of lines.
    fn scroll_help_down(&mut self, lines: usize, total_lines: usize);

    /// Scroll the help panel to the top.
    fn scroll_help_top(&mut self);

    /// Scroll the help panel to the bottom.
    fn scroll_help_bottom(&mut self, total_lines: usize);

    /// Get the number of visible help lines.
    fn help_visible_lines(&self) -> usize;

    /// Get the total number of help lines.
    fn help_total_lines(&self) -> usize;

    /// Get the current help scroll position.
    fn help_scroll(&self) -> usize;

    /// Set the help viewport dimensions.
    fn set_help_visible_lines(&mut self, visible_lines: usize, total_lines: usize);

    /// Get the maximum help scroll position.
    fn max_help_scroll(&self, total_lines: usize) -> usize;

    /// Get the number of visible log lines.
    fn log_visible_lines(&self) -> usize;

    /// Set the number of visible log lines.
    fn set_log_visible_lines(&mut self, lines: usize);
}

// Implementation for App is in app.rs to avoid circular dependencies
