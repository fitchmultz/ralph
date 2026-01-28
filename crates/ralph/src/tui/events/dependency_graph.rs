//! Dependency graph overlay key event handling.
//!
//! Responsibilities:
//! - Handle keyboard input when the dependency graph overlay is active.
//! - Support toggling between dependencies and dependents views.
//! - Support toggling critical path highlighting.
//! - Support closing the overlay to return to the previous mode.
//!
//! Not handled here:
//! - Rendering the dependency graph view (see `tui::render::overlays`).
//! - Computing the dependency graph (handled by `queue::graph`).
//!
//! Invariants/assumptions:
//! - Only plain (non-Ctrl/Alt) characters should trigger view toggles.

use super::types::{AppMode, TuiAction};
use super::{is_plain_char, App};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events in Dependency Graph overlay mode.
pub(super) fn handle_dependency_graph_mode_key(app: &mut App, key: KeyEvent) -> Result<TuiAction> {
    match key.code {
        // Close overlay: Esc, 'd', 'v', or 'q' return to previous mode
        KeyCode::Esc => {
            exit_dependency_graph_mode(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('d') if is_plain_char(&key, 'd') => {
            exit_dependency_graph_mode(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('v') if is_plain_char(&key, 'v') => {
            exit_dependency_graph_mode(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('q') if is_plain_char(&key, 'q') => {
            exit_dependency_graph_mode(app);
            Ok(TuiAction::Continue)
        }
        // Toggle between dependencies and dependents view
        KeyCode::Char('t') if is_plain_char(&key, 't') => {
            toggle_view_mode(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Tab => {
            toggle_view_mode(app);
            Ok(TuiAction::Continue)
        }
        // Toggle critical path highlighting
        KeyCode::Char('c') if is_plain_char(&key, 'c') => {
            toggle_critical_highlight(app);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Exit dependency graph mode and restore the previous application mode.
fn exit_dependency_graph_mode(app: &mut App) {
    if let AppMode::DependencyGraphOverlay { previous_mode, .. } = &app.mode {
        app.mode = *previous_mode.clone();
    }
}

/// Toggle between showing dependencies and dependents.
fn toggle_view_mode(app: &mut App) {
    if let AppMode::DependencyGraphOverlay {
        previous_mode,
        show_dependents,
        highlight_critical,
    } = &app.mode
    {
        app.mode = AppMode::DependencyGraphOverlay {
            previous_mode: previous_mode.clone(),
            show_dependents: !show_dependents,
            highlight_critical: *highlight_critical,
        };
    }
}

/// Toggle critical path highlighting.
fn toggle_critical_highlight(app: &mut App) {
    if let AppMode::DependencyGraphOverlay {
        previous_mode,
        show_dependents,
        highlight_critical,
    } = &app.mode
    {
        app.mode = AppMode::DependencyGraphOverlay {
            previous_mode: previous_mode.clone(),
            show_dependents: *show_dependents,
            highlight_critical: !highlight_critical,
        };
    }
}
