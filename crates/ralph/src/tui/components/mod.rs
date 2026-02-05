//! TUI components built on the foundation layer.
//!
//! Responsibilities:
//! - Provide reusable components for overlays and panels.
//! - Implement the Component trait for specific UI elements.
//!
//! Not handled here:
//! - Low-level focus/layout primitives (see `foundation`).
//! - Application-specific business logic (see individual app_* modules).
//!
//! Invariants/assumptions:
//! - Components are designed to work with the foundation's focus system.
//! - Components can be composed to build complex UIs.

// Allow dead code since these components are meant for future use.
// As more overlays migrate to use the component system, these warnings will go away.
#![allow(dead_code)]

mod util;

mod diff_viewer;
mod line_number_gutter;

pub(crate) mod animation;
pub(crate) mod big_text_header;

pub(crate) mod markdown_renderer;
mod scroll_container;
mod select_list;
mod single_line_input;
mod slider;
pub(crate) mod syntax_highlighter;
mod task_editor_overlay;
mod textarea;
