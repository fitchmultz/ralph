//! Terminal resize handling and UI frame counter for the TUI.
//!
//! Responsibilities:
//! - Handle terminal resize events and update layout-dependent state.
//! - Track UI frame counter for animation timing.
//! - Manage help overlay animation start frame.
//!
//! Not handled here:
//! - Actual rendering (see render module).
//! - Layout computation (handled fresh each frame in render loop).
//!
//! Invariants/assumptions:
//! - Selection and scroll are clamped to valid ranges after resize.
//! - UI frame counter is incremented once per draw cycle.

use crate::tui::App;
use crate::tui::app_panel::PanelOperations;
use crate::tui::app_scroll::ScrollOperations;

/// Trait for resize and UI frame operations.
pub trait ResizeOperations {
    /// Handle terminal resize events.
    ///
    /// Responsibilities:
    /// - Clear cached list_area to force recalculation on next render.
    /// - Clamp scroll positions to ensure they remain valid after terminal resize.
    ///
    /// Not handled here:
    /// - Layout computation (handled fresh each frame in render loop).
    /// - Widget positioning (ratatui handles this via `f.area()`).
    fn handle_resize(&mut self, width: u16, height: u16);

    /// Check if the terminal was resized since the last redraw.
    ///
    /// Returns true if a resize occurred and clears the flag.
    fn take_resized(&mut self) -> bool;

    /// Set the resized flag to trigger layout recalculation.
    fn set_resized(&mut self, width: u16, height: u16);

    /// Get the current UI frame number.
    fn ui_frame(&self) -> u64;

    /// Increment the UI frame counter.
    /// Should be called once per draw cycle.
    fn bump_ui_frame(&mut self);

    /// Get or initialize the help overlay start frame.
    /// Returns the stored start frame if set, otherwise stores and returns `now_frame`.
    fn help_overlay_start_frame(&mut self, now_frame: u64) -> u64;

    /// Clear the help overlay start frame (called when leaving help mode).
    fn clear_help_overlay_start_frame(&mut self);
}

impl ResizeOperations for App {
    fn handle_resize(&mut self, width: u16, height: u16) {
        // Set flag to trigger immediate redraw and layout recalculation
        self.resized = true;

        // Clear cached list_area to force recalculation
        self.clear_list_area();

        // Clamp selection and scroll to valid range for the filtered list
        self.clamp_selection_and_scroll();

        // Clamp help scroll to valid range
        let help_max = self.max_help_scroll(self.help_total_lines);
        if self.help_scroll > help_max {
            self.help_scroll = help_max;
        }

        // Update detail width for text wrapping calculations
        self.detail_width = width.saturating_sub(4);

        // Clamp log scroll to ensure it stays within bounds after resize
        let log_count = self.logs.len();
        if self.log_scroll > log_count {
            self.log_scroll = log_count;
        }

        // Reset ANSI buffer visible lines to trigger recalculation
        if height > 0 {
            self.log_visible_lines = height.saturating_sub(4) as usize;
        }
    }

    fn take_resized(&mut self) -> bool {
        let was_resized = self.resized;
        self.resized = false;
        was_resized
    }

    fn set_resized(&mut self, _width: u16, _height: u16) {
        self.resized = true;
    }

    fn ui_frame(&self) -> u64 {
        self.ui_frame
    }

    fn bump_ui_frame(&mut self) {
        self.ui_frame = self.ui_frame.wrapping_add(1);
    }

    fn help_overlay_start_frame(&mut self, now_frame: u64) -> u64 {
        *self.help_overlay_start_frame.get_or_insert(now_frame)
    }

    fn clear_help_overlay_start_frame(&mut self) {
        self.help_overlay_start_frame = None;
    }
}
