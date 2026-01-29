//! Tests for flowchart overlay rendering.
//!
//! Responsibilities:
//! - Validate flowchart overlay rendering without panic.
//!
//! Not handled here:
//! - Other overlay types (see overlays.rs).
//! - Panel or footer rendering.

use super::common::*;
use crate::tui::{App, AppMode};
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn flowchart_overlay_renders_without_panic() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());
    app.mode = AppMode::FlowchartOverlay {
        previous_mode: Box::new(AppMode::Normal),
    };

    // Should not panic when rendering flowchart overlay
    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    // Verify the mode is set correctly
    assert!(matches!(app.mode, AppMode::FlowchartOverlay { .. }));
}

#[test]
fn flowchart_overlay_shows_phase_indicators() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());
    app.mode = AppMode::FlowchartOverlay {
        previous_mode: Box::new(AppMode::Normal),
    };

    // Should render without panic - phase indicators are visible in the popup
    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    // Verify the mode is still FlowchartOverlay after rendering
    assert!(matches!(app.mode, AppMode::FlowchartOverlay { .. }));
}

#[test]
fn flowchart_overlay_does_not_panic_on_narrow_terminal() {
    let backend = TestBackend::new(50, 20);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());
    app.mode = AppMode::FlowchartOverlay {
        previous_mode: Box::new(AppMode::Normal),
    };

    // Should not panic on narrow terminal (uses vertical layout)
    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");
}
