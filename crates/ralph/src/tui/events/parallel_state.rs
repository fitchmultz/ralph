//! Parallel state overlay key handling.
//!
//! Responsibilities:
//! - Handle keyboard input when the parallel state overlay is active.
//! - Support closing the overlay and returning to the previous mode.
//! - Support reload + PR quick actions (open/copy) in a read-only view.
//!
//! Not handled here:
//! - Rendering (see `tui::render::overlays::parallel_state`).
//! - Performing side effects directly (browser/clipboard are triggered via `TuiAction`).
//!
//! Invariants/assumptions:
//! - Overlay is strictly read-only: it never starts/stops parallel runs and never writes state.
//! - `App` exposes small helper methods for overlay state and for selecting PR URLs.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use super::types::{AppMode, TuiAction};
use super::{App, is_plain_char};

/// Handle key events in Parallel State overlay mode.
pub(super) fn handle_parallel_state_mode_key(app: &mut App, key: KeyEvent) -> Result<TuiAction> {
    match key.code {
        // Close overlay: Esc, 'P', 'h', '?'
        KeyCode::Esc => {
            exit_parallel_state_overlay(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('P') if is_plain_char(&key, 'P') => {
            exit_parallel_state_overlay(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('h') if is_plain_char(&key, 'h') => {
            exit_parallel_state_overlay(app);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('?') if is_plain_char(&key, '?') => {
            exit_parallel_state_overlay(app);
            Ok(TuiAction::Continue)
        }

        // Reload state
        KeyCode::Char('r') if is_plain_char(&key, 'r') => {
            app.parallel_state_overlay_reload_from_disk();
            Ok(TuiAction::Continue)
        }

        // Tab switching
        KeyCode::Tab | KeyCode::Right => {
            app.parallel_state_overlay_next_tab();
            Ok(TuiAction::Continue)
        }
        KeyCode::BackTab | KeyCode::Left => {
            app.parallel_state_overlay_prev_tab();
            Ok(TuiAction::Continue)
        }

        // Navigation / scrolling
        KeyCode::Up => {
            app.parallel_state_overlay_up();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('k') if is_plain_char(&key, 'k') => {
            app.parallel_state_overlay_up();
            Ok(TuiAction::Continue)
        }
        KeyCode::Down => {
            app.parallel_state_overlay_down();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('j') if is_plain_char(&key, 'j') => {
            app.parallel_state_overlay_down();
            Ok(TuiAction::Continue)
        }
        KeyCode::PageUp => {
            app.parallel_state_overlay_page_up();
            Ok(TuiAction::Continue)
        }
        KeyCode::PageDown => {
            app.parallel_state_overlay_page_down();
            Ok(TuiAction::Continue)
        }
        KeyCode::Home => {
            app.parallel_state_overlay_top();
            Ok(TuiAction::Continue)
        }
        KeyCode::End => {
            app.parallel_state_overlay_bottom();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('g') if is_plain_char(&key, 'g') => {
            app.parallel_state_overlay_top();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('G') if is_plain_char(&key, 'G') => {
            app.parallel_state_overlay_bottom();
            Ok(TuiAction::Continue)
        }

        // PR quick actions (no-op unless a PR is selected/available)
        KeyCode::Enter => {
            if let Some(url) = app.parallel_state_overlay_selected_pr_url() {
                return Ok(TuiAction::OpenUrlInBrowser(url.to_string()));
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('o') if is_plain_char(&key, 'o') => {
            if let Some(url) = app.parallel_state_overlay_selected_pr_url() {
                return Ok(TuiAction::OpenUrlInBrowser(url.to_string()));
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('y') if is_plain_char(&key, 'y') => {
            if let Some(url) = app.parallel_state_overlay_selected_pr_url() {
                return Ok(TuiAction::CopyToClipboard(url.to_string()));
            }
            Ok(TuiAction::Continue)
        }

        _ => Ok(TuiAction::Continue),
    }
}

fn exit_parallel_state_overlay(app: &mut App) {
    if let AppMode::ParallelStateOverlay { previous_mode } = &app.mode {
        app.mode = *previous_mode.clone();
    } else {
        app.mode = AppMode::Normal;
    }
}
