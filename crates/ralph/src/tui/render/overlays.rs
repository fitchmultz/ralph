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

use super::super::config_edit::RiskLevel;
use super::super::{help, App, ConfigFieldKind, TaskEditKind, TextInput};
use super::utils::scroll_indicator;
use crate::outpututil::truncate_chars;
use crate::tui::events::types::{TaskBuilderState, TaskBuilderStep};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Draw of full-screen help overlay with keybindings.
pub(super) fn draw_help_overlay(f: &mut Frame<'_>, app: &mut App, area: Rect) {
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
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

/// Draw command palette overlay.
pub(super) fn draw_command_palette(
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

/// Draw risky config confirmation dialog.
pub(super) fn draw_risky_config_dialog(f: &mut Frame<'_>, area: Rect, warning: &str) {
    // Calculate dimensions based on warning text
    let lines: Vec<&str> = warning.lines().collect();
    let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let popup_width = (max_line_len as u16 + 8)
        .min(area.width.saturating_sub(4))
        .max(44);
    let popup_height = (lines.len() as u16 + 6).min(area.height).max(6);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

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
pub(super) fn draw_revert_dialog(
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
    if allow_proceed {
        4
    } else {
        3
    }
}

/// Draw config editor overlay.
pub(super) fn draw_config_editor(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    selected: usize,
    editing_value: Option<&TextInput>,
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
                    value = editing.with_cursor_marker('_');
                }
            }
            let label = format!("{:label_width$}", entry.label);

            // Build line with optional warning indicator
            let (line_text, warning_style) = if entry.risk_level == RiskLevel::Danger {
                let warning_icon = "⚠ ";
                let text = format!("{}{} {}", label, value, warning_icon);
                (text, Some(Color::Red))
            } else if entry.risk_level == RiskLevel::Warning {
                let info_icon = "ℹ ";
                let text = format!("{}{} {}", label, value, info_icon);
                (text, Some(Color::Yellow))
            } else {
                let text = format!("{} {}", label, value);
                (text, None)
            };

            let display = truncate_chars(&line_text, list_area.width as usize);

            let mut style = Style::default();
            if entry.value == "(global default)" {
                style = style.fg(Color::DarkGray);
            }
            if let Some(color) = warning_style {
                style = style.fg(color);
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
    editing_value: Option<&TextInput>,
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
                            value = editing.with_cursor_marker('_');
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

/// Draw the advanced task builder overlay.
pub(super) fn draw_task_builder(f: &mut Frame<'_>, area: Rect, state: &TaskBuilderState) {
    match state.step {
        TaskBuilderStep::Description => draw_task_builder_description(f, area, state),
        TaskBuilderStep::Advanced => draw_task_builder_advanced(f, area, state),
    }
}

/// Draw the description input step of the task builder.
fn draw_task_builder_description(f: &mut Frame<'_>, area: Rect, state: &TaskBuilderState) {
    let popup_width = 70.min(area.width.saturating_sub(4)).max(50);
    let popup_height = 10.min(area.height);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![Span::styled(
            "Build Task with Agent",
            Style::default().add_modifier(Modifier::BOLD),
        )]));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(inner);

    // Prompt
    let prompt = Paragraph::new(Line::from(vec![Span::raw("Enter task description:")]));
    f.render_widget(prompt, chunks[0]);

    // Input field
    let input_text = state.description_input.with_cursor_marker('_');
    let input = Paragraph::new(Line::from(vec![Span::styled(
        input_text,
        Style::default().fg(Color::Yellow),
    )]))
    .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(input, chunks[1]);

    // Error message or hint
    let hint_text = if let Some(ref error) = state.error_message {
        Line::from(vec![Span::styled(error, Style::default().fg(Color::Red))])
    } else {
        Line::from(vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":continue "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ])
    };
    let hint = Paragraph::new(hint_text).alignment(Alignment::Center);
    f.render_widget(hint, chunks[2]);
}

/// Draw the advanced options step of the task builder.
fn draw_task_builder_advanced(f: &mut Frame<'_>, area: Rect, state: &TaskBuilderState) {
    let popup_width = 86.min(area.width.saturating_sub(4)).max(60);
    let popup_height = 18.min(area.height);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let title = Line::from(vec![
        Span::styled(
            "Build Task with Agent",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("(advanced)", Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)].as_ref())
        .split(inner);

    let list_area = layout[0];
    let hint_area = layout[1];

    let label_width = 20usize;

    // Build list items for each field
    let fields = [
        ("Tags hint", format_field_value(&state.tags_hint), 0usize),
        ("Scope hint", format_field_value(&state.scope_hint), 1usize),
        ("Runner", format_runner(state.runner_override), 2usize),
        (
            "Model",
            format_field_value(&state.model_override_input),
            3usize,
        ),
        (
            "Reasoning effort",
            format_effort(state.effort_override),
            4usize,
        ),
        (
            "RepoPrompt mode",
            format_repoprompt(state.repoprompt_mode),
            5usize,
        ),
        ("[ Build Task ]", String::new(), 6usize),
    ];

    let items: Vec<ListItem> = fields
        .iter()
        .map(|(label, value, idx)| {
            let is_selected = *idx == state.selected_field;
            let label_str = format!("{:label_width$}", label);
            let line_text = if value.is_empty() {
                label_str
            } else {
                format!("{} {}", label_str, value)
            };
            let display = truncate_chars(&line_text, list_area.width as usize);

            let mut style = Style::default();
            if value == "(use config default)" {
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

    // Error message or hint
    let hint_text = if let Some(ref error) = state.error_message {
        Text::from(vec![Line::from(vec![Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        )])])
    } else {
        Text::from(vec![
            Line::from(vec![
                Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":nav "),
                Span::styled("Space/Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":cycle "),
                Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":clear "),
            ]),
            Line::from(vec![
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":cancel "),
                Span::styled("type", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":edit text"),
            ]),
        ])
    };
    let hint = Paragraph::new(hint_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint, hint_area);
}

/// Format a field value for display.
fn format_field_value(value: &str) -> String {
    if value.is_empty() {
        "(use config default)".to_string()
    } else {
        value.to_string()
    }
}

/// Format a runner option for display.
fn format_runner(runner: Option<crate::contracts::Runner>) -> String {
    match runner {
        None => "(use config default)".to_string(),
        Some(r) => format!("{:?}", r).to_lowercase(),
    }
}

/// Format a reasoning effort option for display.
fn format_effort(effort: Option<crate::contracts::ReasoningEffort>) -> String {
    match effort {
        None => "(use config default)".to_string(),
        Some(crate::contracts::ReasoningEffort::XHigh) => "xhigh".to_string(),
        Some(e) => format!("{:?}", e).to_lowercase(),
    }
}

/// Format a RepoPrompt mode option for display.
fn format_repoprompt(mode: Option<crate::agent::RepoPromptMode>) -> String {
    match mode {
        None => "(use config default)".to_string(),
        Some(crate::agent::RepoPromptMode::Tools) => "tools".to_string(),
        Some(crate::agent::RepoPromptMode::Plan) => "plan".to_string(),
        Some(crate::agent::RepoPromptMode::Off) => "off".to_string(),
    }
}
