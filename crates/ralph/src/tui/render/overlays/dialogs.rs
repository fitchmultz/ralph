//! Dialog overlay rendering (confirmations and warnings).
//!
//! Responsibilities:
//! - Render confirmation dialogs for destructive actions (delete, archive, quit, discard).
//! - Render risky config warning dialogs.
//! - Render revert confirmation dialogs with multiple options.
//!
//! Not handled here:
//! - Dialog state management or action execution (handled by app event loop).
//! - Input handling for dialog navigation (handled by app event loop).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - Dialogs are centered and sized appropriately for their content.

use crate::tui::TextInput;
use crate::tui::foundation::centered;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

/// Draw confirmation dialog for a destructive action.
pub fn draw_confirm_dialog(f: &mut Frame<'_>, area: Rect, message: &str, hint: &str) {
    let popup_width = 44.min(area.width.saturating_sub(4));
    // Clamp to available height to avoid drawing outside the frame on tiny terminals.
    let popup_height = 6.min(area.height);

    let popup_area = centered(area, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let popup = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(message, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(hint, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray)),
    )
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: false });

    f.render_widget(popup, popup_area);
}

/// Draw risky config confirmation dialog.
pub fn draw_risky_config_dialog(f: &mut Frame<'_>, area: Rect, warning: &str) {
    // Calculate dimensions based on warning text
    let lines: Vec<&str> = warning.lines().collect();
    let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let popup_width = (max_line_len as u16 + 8)
        .min(area.width.saturating_sub(4))
        .max(44);
    let popup_height = (lines.len() as u16 + 6).min(area.height).max(6);

    let popup_area = centered(area, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let mut text_lines: Vec<Line> = vec![Line::from("")];

    for line in lines {
        let style = if line.starts_with('⚠') {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        text_lines.push(Line::from(Span::styled(line.to_string(), style)));
    }

    text_lines.push(Line::from(""));
    text_lines.push(Line::from(vec![
        Span::styled("Confirm? ", Style::default()),
        Span::styled("(y/n)", Style::default().fg(Color::Yellow)),
    ]));
    text_lines.push(Line::from(""));

    let popup = Paragraph::new(Text::from(text_lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Warning")
                .style(Style::default().bg(Color::DarkGray)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });

    f.render_widget(popup, popup_area);
}

/// Draw revert confirmation dialog.
pub fn draw_revert_dialog(
    f: &mut Frame<'_>,
    area: Rect,
    label: &str,
    preface: Option<&str>,
    allow_proceed: bool,
    selected: usize,
    input: &TextInput,
) {
    let popup_width = 64.min(area.width.saturating_sub(4));
    let preface_lines = preface.map(|text| text.lines().count()).unwrap_or(0);
    let base_height =
        7 + options_len(allow_proceed) + preface_lines + if preface_lines > 0 { 1 } else { 0 };
    // Clamp to available height to avoid drawing outside the frame on tiny terminals.
    let popup_height = (base_height as u16).min(area.height).max(8);

    let popup_area = centered(area, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let highlight = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let normal = Style::default();

    let mut options = vec![
        "1) Keep (default)".to_string(),
        "2) Revert".to_string(),
        "3) Other (type message)".to_string(),
    ];
    if allow_proceed {
        options.push("4) Keep + continue".to_string());
    }

    let mut lines = Vec::new();
    lines.push(Line::from(""));
    if let Some(preface) = preface {
        for line in preface.lines() {
            if line.is_empty() {
                lines.push(Line::from(""));
            } else {
                lines.push(Line::from(Span::raw(line.to_string())));
            }
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("{label}: action?"),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    for (idx, text) in options.iter().enumerate() {
        let style = if idx == selected { highlight } else { normal };
        lines.push(Line::from(Span::styled((*text).to_string(), style)));
    }

    lines.push(Line::from(""));
    let message_line = if selected == 2 {
        format!("Message: {}", input.with_cursor_marker('_'))
    } else {
        "Message: (select Other to type)".to_string()
    };
    lines.push(Line::from(Span::styled(
        message_line,
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("↑↓/jk", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":select "),
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":confirm "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":keep"),
    ]));

    let popup = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray)),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });

    f.render_widget(popup, popup_area);
}

fn options_len(allow_proceed: bool) -> usize {
    if allow_proceed { 4 } else { 3 }
}
