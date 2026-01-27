//! TUI modal/overlay rendering helpers.
//!
//! Responsibilities:
//! - Render modal overlays such as help, palettes, editors, and confirmations.
//! - Keep overlay layout consistent with TUI styling conventions.
//!
//! Not handled here:
//! - Event handling for overlay interaction.
//! - Base layout panels or footer rendering.
//!
//! Invariants/assumptions:
//! - Callers provide terminal areas sized for the current frame.
//! - Overlay drawing clears the underlying area before rendering content.

use super::super::{keymap, App, ConfigFieldKind, TaskEditKind};
use crate::outpututil::truncate_chars;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

#[derive(Debug, Clone)]
enum HelpLine {
    Title(&'static str),
    Section(&'static str),
    Text(String),
    Blank,
}

/// Draw of full-screen help overlay with keybindings.
pub(super) fn draw_help_overlay(f: &mut Frame<'_>, area: Rect) {
    let popup = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    f.render_widget(Clear, popup);

    let block = Block::default()
        .title(Line::from(Span::styled(
            "Help",
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .borders(Borders::ALL);
    f.render_widget(block, popup);

    let inner = popup.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    let lines = help_overlay_lines();

    let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

fn help_overlay_lines() -> Vec<Line<'static>> {
    build_help_overlay_lines()
        .into_iter()
        .map(|line| match line {
            HelpLine::Title(text) | HelpLine::Section(text) => Line::from(Span::styled(
                text,
                Style::default().add_modifier(Modifier::BOLD),
            )),
            HelpLine::Text(text) => Line::from(text),
            HelpLine::Blank => Line::from(""),
        })
        .collect()
}

#[cfg(test)]
pub(super) fn help_overlay_plain_lines() -> Vec<String> {
    build_help_overlay_lines()
        .into_iter()
        .map(|line| match line {
            HelpLine::Title(text) | HelpLine::Section(text) => text.to_string(),
            HelpLine::Text(text) => text,
            HelpLine::Blank => String::new(),
        })
        .collect()
}

fn build_help_overlay_lines() -> Vec<HelpLine> {
    let mut lines = Vec::new();
    lines.push(HelpLine::Title("Keybindings"));
    let close_keys = join_keys(keymap::help_close_keys());
    lines.push(HelpLine::Text(format!("Press {close_keys} to close.")));
    lines.push(HelpLine::Blank);

    for section in keymap::normal_sections() {
        push_section_lines(&mut lines, section);
    }

    for section in keymap::executing_sections() {
        push_section_lines(&mut lines, section);
    }

    while matches!(lines.last(), Some(HelpLine::Blank)) {
        lines.pop();
    }

    lines
}

fn push_section_lines(lines: &mut Vec<HelpLine>, section: &keymap::KeymapSection) {
    lines.push(HelpLine::Section(section.title));
    for binding in section.bindings {
        lines.push(HelpLine::Text(format!(
            "{}: {}",
            binding.keys_display, binding.description
        )));
    }
    lines.push(HelpLine::Blank);
}

fn join_keys(keys: &[&str]) -> String {
    match keys.len() {
        0 => String::new(),
        1 => keys[0].to_string(),
        2 => format!("{} or {}", keys[0], keys[1]),
        _ => {
            let mut out = String::new();
            for (idx, key) in keys.iter().enumerate() {
                if idx > 0 {
                    if idx == keys.len() - 1 {
                        out.push_str(" or ");
                    } else {
                        out.push_str(", ");
                    }
                }
                out.push_str(key);
            }
            out
        }
    }
}

/// Draw command palette overlay.
pub(super) fn draw_command_palette(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    query: &str,
    selected: usize,
) {
    let entries = app.palette_entries(query);

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

    let input = Line::from(vec![
        Span::styled(
            ":",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        // Avoid allocating a new String every frame.
        Span::styled(query, Style::default().fg(Color::Yellow)),
        Span::styled("_", Style::default().fg(Color::Yellow)),
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

/// Draw confirmation dialog for a destructive action.
pub(super) fn draw_confirm_dialog(f: &mut Frame<'_>, area: Rect, message: &str, hint: &str) {
    let popup_width = 44.min(area.width.saturating_sub(4));
    // Clamp to available height to avoid drawing outside the frame on tiny terminals.
    let popup_height = 6.min(area.height);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

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

/// Draw revert confirmation dialog.
pub(super) fn draw_revert_dialog(
    f: &mut Frame<'_>,
    area: Rect,
    label: &str,
    allow_proceed: bool,
    selected: usize,
    input: &str,
) {
    let popup_width = 64.min(area.width.saturating_sub(4));
    // Clamp to available height to avoid drawing outside the frame on tiny terminals.
    let popup_height = 12.min(area.height);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

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
        format!("Message: {}", input)
    } else {
        "Message: (select Other to type)".to_string()
    };
    lines.push(Line::from(Span::styled(
        message_line,
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Up/Down", Style::default().add_modifier(Modifier::BOLD)),
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

/// Draw config editor overlay.
pub(super) fn draw_config_editor(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    selected: usize,
    editing_value: Option<&str>,
) {
    let entries = app.config_entries();
    if entries.is_empty() {
        return;
    }

    let popup_width = 86.min(area.width.saturating_sub(4)).max(40);
    let popup_height = (entries.len() as u16 + 6)
        .min(area.height.saturating_sub(4))
        .max(8);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let title = Line::from(vec![
        Span::styled(
            "Project Config",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("(.ralph/config.json)", Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(inner);

    let list_area = layout[0];
    let hint_area = layout[1];

    let label_width = 24usize;

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .take(list_area.height as usize)
        .map(|(idx, entry)| {
            let is_selected = idx == selected;
            let mut value = entry.value.clone();
            if is_selected && entry.kind == ConfigFieldKind::Text {
                if let Some(editing) = editing_value {
                    value = format!("{}_", editing);
                }
            }
            let label = format!("{:label_width$}", entry.label);
            let line_text = format!("{} {}", label, value);
            let display = truncate_chars(&line_text, list_area.width as usize);

            let mut style = Style::default();
            if entry.value == "(global default)" {
                style = style.fg(Color::DarkGray);
            }
            if is_selected {
                style = style.bg(Color::Blue).add_modifier(Modifier::BOLD);
            }

            ListItem::new(Line::from(Span::styled(display, style)))
        })
        .collect();

    let list = List::new(items).block(Block::default());
    f.render_widget(list, list_area);

    let hint = Line::from(vec![
        Span::styled("Enter/Space", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":edit "),
        Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":clear "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":close"),
    ]);
    f.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        hint_area,
    );
}

/// Draw task editor overlay.
pub(super) fn draw_task_editor(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    selected: usize,
    editing_value: Option<&str>,
) {
    let entries = app.task_edit_entries();
    if entries.is_empty() {
        return;
    }

    let popup_width = 96.min(area.width.saturating_sub(4)).max(44);
    let popup_height = (entries.len() as u16 + 7)
        .min(area.height.saturating_sub(4))
        .max(9);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let title = Line::from(vec![Span::styled(
        "Task Editor",
        Style::default().add_modifier(Modifier::BOLD),
    )]);

    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)].as_ref())
        .split(inner);

    let list_area = layout[0];
    let hint_area = layout[1];

    let label_width = 18usize;

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .take(list_area.height as usize)
        .map(|(idx, entry)| {
            let is_selected = idx == selected;
            let mut value = entry.value.clone();
            if is_selected {
                match entry.kind {
                    TaskEditKind::Cycle => {}
                    TaskEditKind::Text
                    | TaskEditKind::List
                    | TaskEditKind::Map
                    | TaskEditKind::OptionalText => {
                        if let Some(editing) = editing_value {
                            value = format!("{}_", editing);
                        }
                    }
                }
            }
            let label = format!("{:label_width$}", entry.label);
            let line_text = format!("{} {}", label, value);
            let display = truncate_chars(&line_text, list_area.width as usize);

            let mut style = Style::default();
            if entry.value == "(empty)" {
                style = style.fg(Color::DarkGray);
            }
            if is_selected {
                style = style.bg(Color::Blue).add_modifier(Modifier::BOLD);
            }

            ListItem::new(Line::from(Span::styled(display, style)))
        })
        .collect();

    let list = List::new(items).block(Block::default());
    f.render_widget(list, list_area);

    let hint = Line::from(vec![
        Span::styled("Enter/Space", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":edit "),
        Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":nav "),
        Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":clear "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":close"),
    ]);
    let format_hint = Line::from(vec![
        Span::styled("lists", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(": a, b, c  "),
        Span::styled("maps", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(": key=value"),
    ]);
    let hint_paragraph = Paragraph::new(Text::from(vec![hint, format_hint]))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint_paragraph, hint_area);
}
