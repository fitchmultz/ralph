//! Help overlay state management for the TUI.
//!
//! Responsibilities:
//! - Track scroll position and visible/total line counts for the help overlay.
//! - Store the previous mode to return to when exiting help.
//! - Provide scrolling methods (up, down, top, bottom).
//!
//! Not handled here:
//! - Help content rendering (handled by render module).
//! - Key event handling for help mode (handled by events module).
//! - Help text generation (handled by help module).
//!
//! Invariants/assumptions:
//! - `previous_mode` is always set when entering help mode.
//! - Scroll positions are clamped to valid ranges based on visible/total lines.
//! - `visible_lines` and `total_lines` are updated by the renderer after layout.

#![allow(dead_code)]

use crate::tui::events::AppMode;

/// State for the help overlay.
#[derive(Debug)]
pub struct HelpState {
    /// Scroll offset for the help overlay.
    scroll: usize,
    /// Last known visible help lines in Help overlay (for paging).
    visible_lines: usize,
    /// Last known total help line count (post-wrap).
    total_lines: usize,
    /// Previous mode before entering the Help overlay.
    previous_mode: Option<AppMode>,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            scroll: 0,
            visible_lines: 1,
            total_lines: 0,
            previous_mode: None,
        }
    }
}

impl HelpState {
    /// Create a new help state with default values.
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

    /// Get the previous mode, if any.
    pub fn previous_mode(&self) -> Option<&AppMode> {
        self.previous_mode.as_ref()
    }

    /// Calculate the maximum valid scroll position.
    pub fn max_scroll(&self, total_lines: usize) -> usize {
        total_lines.saturating_sub(self.visible_lines())
    }

    /// Update the visible and total line counts, clamping scroll if necessary.
    pub fn set_viewport(&mut self, visible_lines: usize, total_lines: usize) {
        let visible_lines = visible_lines.max(1);
        self.visible_lines = visible_lines;
        self.total_lines = total_lines;
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

    /// Enter help mode, storing the current mode as previous.
    pub fn enter(&mut self, previous_mode: AppMode) {
        self.previous_mode = Some(previous_mode);
        self.scroll = 0;
    }

    /// Exit help mode, returning the previous mode (or Normal if none).
    pub fn exit(&mut self) -> AppMode {
        self.previous_mode.take().unwrap_or(AppMode::Normal)
    }

    /// Reset the state.
    pub fn reset(&mut self) {
        self.scroll = 0;
        self.visible_lines = 1;
        self.total_lines = 0;
        self.previous_mode = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_clamping() {
        let mut state = HelpState::new();
        state.set_viewport(10, 100);

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
    fn test_viewport_update_clamps_scroll() {
        let mut state = HelpState::new();
        state.set_viewport(20, 100);
        state.scroll_down(80, 100); // Scroll to 80 (max with 20 visible is 80)
        assert_eq!(state.scroll(), 80);

        // Reduce visible lines, scroll should be clamped to new max (90)
        // Since 80 <= 90, no clamping should occur
        state.set_viewport(10, 100);
        assert_eq!(state.scroll(), 80); // Still 80 since it's valid

        // Now test actual clamping - scroll to max then reduce viewport
        state.scroll_down(20, 100); // Try to scroll more, but max is 90
        assert_eq!(state.scroll(), 90); // Clamped to max

        // Reduce visible lines significantly - scroll should be clamped
        state.set_viewport(5, 100);
        // scroll is 90, new max is 95 (100 - 5), 90 <= 95 so no clamping
        assert_eq!(state.scroll(), 90); // Still 90 since it's valid

        // Scroll to new max
        state.scroll_down(20, 100);
        assert_eq!(state.scroll(), 95); // Clamped to new max (100 - 5 = 95)
    }

    #[test]
    fn test_enter_exit_mode() {
        let mut state = HelpState::new();

        state.enter(AppMode::Normal);
        assert_eq!(state.previous_mode(), Some(&AppMode::Normal));
        assert_eq!(state.scroll(), 0);

        let mode = state.exit();
        assert_eq!(mode, AppMode::Normal);
        assert_eq!(state.previous_mode(), None);
    }

    #[test]
    fn test_exit_without_previous() {
        let mut state = HelpState::new();
        let mode = state.exit();
        assert_eq!(mode, AppMode::Normal);
    }
}
