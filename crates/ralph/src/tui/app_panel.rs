//! Panel focus and layout management for the TUI.
//!
//! Responsibilities:
//! - Track which panel is focused (List vs Details)
//! - Manage list area for hit-testing mouse events
//! - Handle terminal resize and coordinate layout recalculation
//!
//! Not handled here:
//! - Actual rendering (see render module)
//! - Panel content/layout details (see render/panels)
//! - Event handling (see events module)
//!
//! Invariants/assumptions:
//! - Panel focus cycles between List and Details only
//! - List area is updated each render pass for accurate hit-testing

use ratatui::layout::Rect;

/// Which panel is currently focused for input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPanel {
    List,
    Details,
}

impl FocusedPanel {
    pub fn next(self) -> Self {
        match self {
            Self::List => Self::Details,
            Self::Details => Self::List,
        }
    }

    pub fn previous(self) -> Self {
        self.next()
    }
}

/// Trait for panel focus operations.
pub trait PanelOperations {
    /// Move focus to the next panel (cycles List -> Details -> List).
    fn focus_next_panel(&mut self);

    /// Move focus to the previous panel (same as next for 2 panels).
    fn focus_previous_panel(&mut self);

    /// Set focus to the list panel.
    fn focus_list_panel(&mut self);

    /// Check if the details panel is currently focused.
    fn details_focused(&self) -> bool;

    /// Set the cached list area rectangle.
    fn set_list_area(&mut self, area: Rect);

    /// Clear the cached list area.
    fn clear_list_area(&mut self);

    /// Get the cached list area, if any.
    fn list_area(&self) -> Option<Rect>;
}

// Implementation for App is in app.rs to avoid circular dependencies
