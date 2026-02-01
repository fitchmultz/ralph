//! Shared rendering harness helpers for TUI integration tests.
//!
//! This module centralizes `ratatui::backend::TestBackend` setup and
//! "render entire UI to a string" plumbing, so individual test modules can
//! focus on state setup + assertions.

#![allow(dead_code)]

use ralph::tui::{self, App};
use ratatui::{Terminal, backend::TestBackend};

/// Setup a `ratatui` test terminal with given dimensions.
pub(crate) fn setup_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).expect("failed to create terminal")
}

/// Render the full TUI into a plain string for substring-based assertions.
pub(crate) fn get_rendered_output(terminal: &mut Terminal<TestBackend>, app: &mut App) -> String {
    terminal
        .draw(|f| {
            // Update detail width from current terminal size.
            // This mirrors the real app behavior where the details panel adapts to terminal width.
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, app)
        })
        .expect("failed to draw");

    let buffer = terminal.backend().buffer();
    let area = buffer.area();

    let mut output = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            let pos = ratatui::layout::Position { x, y };
            let cell = &buffer[pos];
            let symbol = cell.symbol();
            if symbol.is_empty() {
                output.push(' ');
            } else {
                output.push_str(symbol);
            }
        }
        output.push('\n');
    }
    output
}
