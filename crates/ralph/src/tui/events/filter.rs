//! Filter input key handling for the TUI.
//!
//! Responsibilities:
//! - Accept tag/scope filter input and apply changes to `App`.
//! - Handle submit and cancel flow for filter inputs.
//!
//! Not handled here:
//! - Rendering of filter prompts or validation beyond parsing.
//! - Shortcut handling outside filter modes.
//!
//! Invariants/assumptions:
//! - Input uses cursor-aware `TextInput` edits.
//! - On submit or cancel, the mode returns to Normal.

use super::super::{AppMode, TextInput};
use super::types::TuiAction;
use super::{App, handle_filter_input_key};
use anyhow::Result;
use crossterm::event::KeyEvent;

fn set_filter_tags_mode(input: TextInput) -> AppMode {
    AppMode::FilteringTags(input)
}

fn set_filter_scopes_mode(input: TextInput) -> AppMode {
    AppMode::FilteringScopes(input)
}

fn apply_tag_filters(app: &mut App, value: &str) {
    let tags = App::parse_tags(value);
    app.set_tag_filters(tags);
}

fn apply_scope_filters(app: &mut App, value: &str) {
    let scopes = App::parse_list(value);
    app.set_scope_filters(scopes);
}

/// Handle key events in FilteringTags mode.
pub(super) fn handle_filtering_tags_key(
    app: &mut App,
    key: KeyEvent,
    current: TextInput,
) -> Result<TuiAction> {
    handle_filter_input_key(app, key, current, set_filter_tags_mode, apply_tag_filters)
}

pub(super) fn handle_filtering_scopes_key(
    app: &mut App,
    key: KeyEvent,
    current: TextInput,
) -> Result<TuiAction> {
    handle_filter_input_key(
        app,
        key,
        current,
        set_filter_scopes_mode,
        apply_scope_filters,
    )
}
