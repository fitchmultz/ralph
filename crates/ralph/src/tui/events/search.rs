//! Search input key handling for the TUI.
//!
//! Responsibilities:
//! - Capture search query input and apply it to filters.
//! - Exit search mode on submit or cancel.
//!
//! Not handled here:
//! - Rendering search UI.
//! - Regex validation or search execution details.
//!
//! Invariants/assumptions:
//! - Search input uses cursor-aware `TextInput` updates.

use super::super::{AppMode, TextInput};
use super::types::TuiAction;
use super::{App, handle_filter_input_key};
use crate::tui::app_filters::FilterManagementOperations;
use anyhow::Result;
use crossterm::event::KeyEvent;

fn set_search_mode(input: TextInput) -> AppMode {
    AppMode::Searching(input)
}

fn apply_search_query(app: &mut App, value: &str) {
    app.set_search_query(value.to_string());
}

/// Handle key events in Searching mode.
pub(super) fn handle_searching_mode_key(
    app: &mut App,
    key: KeyEvent,
    current: TextInput,
) -> Result<TuiAction> {
    handle_filter_input_key(app, key, current, set_search_mode, apply_search_query)
}
