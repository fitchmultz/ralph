//! Config and task editor overlay rendering.
//!
//! Responsibilities:
//! - Render project config editor overlay with field selection and editing.
//! - Render task field editor overlay with various field types (text, list, map, cycle).
//!
//! Not handled here:
//! - Config/task entry generation (handled by `App::config_entries` and `App::task_edit_entries`).
//! - Input handling and value editing (handled by app event loop).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - Editing values are tracked via `MultiLineInput` state for multi-line editing.

use crate::outpututil::truncate_chars;
use crate::tui::config_edit::RiskLevel;
use crate::tui::foundation::{Item, ItemSize, centered, col};
use crate::tui::{App, MultiLineInput};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

/// Draw config editor overlay.
pub fn draw_config_editor(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    selected: usize,
    editing_value: Option<&MultiLineInput>,
) {
    let entries = app.config_entries();
    if entries.is_empty() {
        return;
    }

    let popup_width = 86.min(area.width.saturating_sub(4)).max(40);
    let popup_height = (entries.len() as u16 + 6)
        .min(area.height.saturating_sub(4))
        .max(8);

    let popup_area = centered(area, popup_width, popup_height);

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
    let layout = col(
        inner,
        0,
        &[Item::new(ItemSize::Min(1)), Item::new(ItemSize::Fixed(1))],
    );

    let list_area = layout[0];
    let hint_area = layout[1];

    let label_width = 24usize;

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .take(list_area.height as usize)
        .map(|(idx, entry)| {
            let is_selected = idx == selected;
            let value = entry.value.clone();
            // Note: editing_value is now rendered separately as a textarea overlay
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

    // Render textarea overlay when editing
    if let Some(textarea) = editing_value {
        let edit_area = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 2 + selected as u16,
            width: popup_width.saturating_sub(4),
            height: 6.min(popup_height.saturating_sub(4)),
        };
        f.render_widget(textarea.widget(), edit_area);
    }

    let hint = Line::from(vec![
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":commit "),
        Span::styled("Alt+Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":newline "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":cancel"),
    ]);
    f.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        hint_area,
    );
}

/// Draw task editor overlay.
pub fn draw_task_editor(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    selected: usize,
    editing_value: Option<&MultiLineInput>,
) {
    let entries = app.task_edit_entries();
    if entries.is_empty() {
        return;
    }

    let popup_width = 96.min(area.width.saturating_sub(4)).max(44);
    let popup_height = (entries.len() as u16 + 7)
        .min(area.height.saturating_sub(4))
        .max(9);

    let popup_area = centered(area, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let title = Line::from(vec![Span::styled(
        "Task Editor",
        Style::default().add_modifier(Modifier::BOLD),
    )]);

    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);
    let layout = col(
        inner,
        0,
        &[Item::new(ItemSize::Min(1)), Item::new(ItemSize::Fixed(2))],
    );

    let list_area = layout[0];
    let hint_area = layout[1];

    let label_width = 18usize;

    // Check if we're currently editing
    let is_editing = editing_value.is_some();

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .take(list_area.height as usize)
        .map(|(idx, entry)| {
            let is_selected = idx == selected;
            let value = if is_selected && is_editing {
                // Show placeholder when editing (actual textarea renders separately)
                "...".to_string()
            } else {
                entry.value.clone()
            };
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

    // Render textarea overlay when editing
    if let Some(textarea) = editing_value {
        let edit_area = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 2 + selected as u16,
            width: popup_width.saturating_sub(4),
            height: 6.min(popup_height.saturating_sub(4)),
        };
        f.render_widget(textarea.widget(), edit_area);
    }

    let hint = if is_editing {
        Line::from(vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":commit "),
            Span::styled("Alt+Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":newline "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter/Space", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":edit "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":nav "),
            Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":clear "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":close"),
        ])
    };
    let format_hint = Line::from(vec![
        Span::styled("lists", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(": one item per line  "),
        Span::styled("maps", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(": key=value"),
    ]);
    let hint_paragraph = Paragraph::new(ratatui::text::Text::from(vec![hint, format_hint]))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint_paragraph, hint_area);
}
