//! Panel focus and layout management for the TUI.
//!
//! Responsibilities:
//! - Define focus IDs for base panels (List/Details) and the operations for switching.
//! - Provide list area caching for row hit-testing (selection logic remains separate).
//!
//! Not handled here:
//! - Rendering (see `tui::render`).
//! - Overlay focus (handled by foundation `FocusScope::Overlay`).
//!
//! Invariants/assumptions:
//! - Base panel focus is driven by `FocusManager`.
//! - Details focus may be disabled when the details panel is not visible.

use crate::tui::foundation::{ComponentId, FocusId};
use ratatui::layout::Rect;

/// Component ID for the base panels component.
pub(crate) const PANELS_COMPONENT: ComponentId = ComponentId::new("base_panels", 0);

/// Focus ID for the list panel.
pub(crate) const LIST_PANEL_FOCUS: FocusId = FocusId::new(PANELS_COMPONENT, 0);

/// Focus ID for the details panel.
pub(crate) const DETAILS_PANEL_FOCUS: FocusId = FocusId::new(PANELS_COMPONENT, 1);

/// Which panel is currently focused for input.
///
/// Deprecated: Use foundation `FocusManager` with `LIST_PANEL_FOCUS` and
/// `DETAILS_PANEL_FOCUS` instead. Kept for backward compatibility during migration.
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
