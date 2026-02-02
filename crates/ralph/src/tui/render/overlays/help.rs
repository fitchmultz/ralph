//! Help overlay rendering.
//!
//! Responsibilities:
//! - Render full-screen help overlay with keybindings and scrollable content.
//! - Display scroll indicators when content exceeds visible area.
//!
//! Not handled here:
//! - Help content generation (handled by `super::super::help`).
//! - Event handling for scrolling (handled by app event loop).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - App state tracks help scroll position via `set_help_visible_lines`.

use crate::tui::App;
use crate::tui::help;
use crate::tui::render::utils::scroll_indicator;
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Draw full-screen help overlay with keybindings.
pub fn draw_help_overlay(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let popup = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    f.render_widget(Clear, popup);

    let inner = popup.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let content_width = inner.width as usize;
    let total_lines = help::help_line_count(content_width);
    let visible_lines = inner.height as usize;
    app.set_help_visible_lines(visible_lines, total_lines);

    let indicator = scroll_indicator(app.help_scroll(), app.help_visible_lines(), total_lines);
    let block = Block::default()
        .title(help_title(indicator))
        .borders(Borders::ALL);
    f.render_widget(block, popup);

    let lines = help::help_overlay_lines(content_width);
    let paragraph = Paragraph::new(Text::from(lines)).scroll((app.help_scroll() as u16, 0));
    f.render_widget(paragraph, inner);
}

fn help_title(indicator: Option<String>) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "Help",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if let Some(indicator) = indicator {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            indicator,
            Style::default().fg(ratatui::style::Color::DarkGray),
        ));
    }
    Line::from(spans)
}
