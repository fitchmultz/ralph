//! Command palette overlay rendering.
//!
//! Responsibilities:
//! - Render command palette with filtered entries.
//! - Display input field with cursor and scrollable results list.
//!
//! Not handled here:
//! - Palette entry filtering logic (handled by `App::palette_entries`).
//! - Input handling and selection navigation (handled by app event loop).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - Selected index is clamped to valid range.

use crate::tui::{App, TextInput};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

/// Draw command palette overlay.
pub fn draw_command_palette(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    query: &TextInput,
    selected: usize,
) {
    let entries = app.palette_entries(query.value());

    let popup_width = 70.min(area.width.saturating_sub(4));

    // Keep the popup inside the available frame (tiny terminals can be smaller than our min).
    let mut popup_height = (entries.len() as u16)
        .saturating_add(4)
        .min(area.height.saturating_sub(4));
    popup_height = popup_height.max(6).min(area.height);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled("Command", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled("(type to filter)", Style::default().fg(Color::DarkGray)),
        ]));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
        .split(inner);

    let input_text = query.with_cursor_marker('_');
    let input = Line::from(vec![
        Span::styled(
            ":",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(input_text, Style::default().fg(Color::Yellow)),
    ]);
    f.render_widget(Paragraph::new(input), inner_chunks[0]);

    let list_height = inner_chunks[1].height as usize;
    let entry_count = entries.len();
    let selected = selected.min(entry_count.saturating_sub(1));
    let (start, end) = if list_height == 0 || entry_count == 0 {
        (0, 0)
    } else {
        let max_start = entry_count.saturating_sub(list_height);
        let start = selected
            .saturating_sub(list_height.saturating_sub(1))
            .min(max_start);
        let end = (start + list_height).min(entry_count);
        (start, end)
    };
    let visible_entries = &entries[start..end];
    let selected_idx = selected.saturating_sub(start);

    // Borrow entry titles instead of cloning them every draw.
    let items: Vec<ListItem<'_>> = if visible_entries.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "(no matches)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        visible_entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let style = if idx == selected_idx {
                    Style::default()
                        .bg(Color::Blue)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(entry.title.as_str(), style)))
            })
            .collect()
    };

    let list = List::new(items).block(Block::default());
    f.render_widget(list, inner_chunks[1]);
}
