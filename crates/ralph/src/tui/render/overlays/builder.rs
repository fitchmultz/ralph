//! Task builder overlay rendering.
//!
//! Responsibilities:
//! - Render task builder overlay with description input and advanced options.
//! - Handle multi-step builder flow (description -> advanced options -> build).
//!
//! Not handled here:
//! - Task builder state management (handled by `TaskBuilderState`).
//! - Input handling and field navigation (handled by app event loop).
//! - Actual task creation (handled by task builder logic).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - Builder state is properly initialized before rendering.

use crate::agent::RepoPromptMode;
use crate::contracts;
use crate::outpututil::truncate_chars;
use crate::tui::events::types::{TaskBuilderState, TaskBuilderStep};
use crate::tui::foundation::centered;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

/// Draw the advanced task builder overlay.
pub fn draw_task_builder(f: &mut Frame<'_>, area: Rect, state: &TaskBuilderState) {
    match state.step {
        TaskBuilderStep::Description => draw_task_builder_description(f, area, state),
        TaskBuilderStep::Advanced => draw_task_builder_advanced(f, area, state),
    }
}

/// Draw the description input step of the task builder.
fn draw_task_builder_description(f: &mut Frame<'_>, area: Rect, state: &TaskBuilderState) {
    let popup_width = 70.min(area.width.saturating_sub(4)).max(50);
    let popup_height = 10.min(area.height);

    let popup_area = centered(area, popup_width, popup_height);

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

    let popup_area = centered(area, popup_width, popup_height);

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
fn format_runner(runner: Option<contracts::Runner>) -> String {
    match runner {
        None => "(use config default)".to_string(),
        Some(r) => format!("{:?}", r).to_lowercase(),
    }
}

/// Format a reasoning effort option for display.
fn format_effort(effort: Option<contracts::ReasoningEffort>) -> String {
    match effort {
        None => "(use config default)".to_string(),
        Some(contracts::ReasoningEffort::XHigh) => "xhigh".to_string(),
        Some(e) => format!("{:?}", e).to_lowercase(),
    }
}

/// Format a RepoPrompt mode option for display.
fn format_repoprompt(mode: Option<RepoPromptMode>) -> String {
    match mode {
        None => "(use config default)".to_string(),
        Some(RepoPromptMode::Tools) => "tools".to_string(),
        Some(RepoPromptMode::Plan) => "plan".to_string(),
        Some(RepoPromptMode::Off) => "off".to_string(),
    }
}
