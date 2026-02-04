//! TUI foundation layer for component-based architecture, focus management, and layout primitives.
//!
//! Responsibilities:
//! - Define the core component interface for migrated screens/overlays.
//! - Provide deterministic focus management with scopes, wraparound, and hit-testing.
//! - Offer flexbox-like layout primitives built on Ratatui's Rect.
//!
//! Not handled here:
//! - AppMode dispatch (still owned by existing `tui/events/*`).
//! - Global app state ownership (still owned by `App`).
//! - Rendering styles and widgets (components decide their own appearance).
//!
//! Invariants/assumptions:
//! - Rendering is immediate-mode; components must be safe to render every frame.
//! - Focus registration happens during render to ensure rects are current.
//! - Focus order is deterministic (based on traversal path, not registration order).
//! - Layout helpers use saturating math and never panic on small/zero-sized terminals.

// Allow dead code since this is a foundation layer meant for future use.
// As more components migrate to use this foundation, these warnings will go away.
#![allow(dead_code)]

mod component;
mod event;
mod focus;
mod layout;

pub(crate) use component::{Component, RenderCtx};
pub(crate) use event::UiEvent;

// EXPAND: focus exports needed by non-component legacy layers
pub(crate) use focus::{
    ComponentId, FocusId, FocusManager, FocusRegistry, FocusScope, FocusTraversal,
};

// EXPAND: layout exports so callers stop re-implementing centered/pad/etc.
pub(crate) use layout::{Item, ItemSize, centered, col};

#[cfg(test)]
mod tests;
