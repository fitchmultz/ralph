//! Jump to task input overlay rendering.
//!
//! Responsibilities:
//! - Render input overlay for jumping to a task by ID.
//!
//! Not handled here:
//! - Task lookup and navigation (handled by app event loop).
//! - Input handling (handled by app event loop).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - Input state is tracked via `TextInput`.

use crate::tui::TextInput;
use crate::tui::foundation::centered;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Draw jump-to-task input overlay.
pub fn draw_jump_to_task_input(f: &mut Frame<'_>, area: Rect, input: &TextInput) {
    let popup_width = 50.min(area.width.saturating_sub(4)).max(30);
    let popup_height = 5.min(area.height);

    let popup_area = centered(area, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![Span::styled(
            "Jump to Task ID",
            Style::default().add_modifier(Modifier::BOLD),
        )]));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)].as_ref())
        .split(inner);

    // Input field with cursor
    let input_text = input.with_cursor_marker('_');
    let input_line = Line::from(vec![Span::styled(
        input_text,
        Style::default().fg(Color::Yellow),
    )]);
    f.render_widget(Paragraph::new(input_line), chunks[0]);

    // Hint
    let hint = Line::from(vec![
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":jump "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":cancel"),
    ]);
    f.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        chunks[1],
    );
}
