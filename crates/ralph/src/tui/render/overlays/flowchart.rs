//! Workflow flowchart overlay rendering.
//!
//! Responsibilities:
//! - Render workflow flowchart showing 3-phase execution progress.
//! - Support both horizontal (wide) and vertical (narrow) layouts.
//! - Display phase status with icons and timing information.
//!
//! Not handled here:
//! - Phase state management (handled by `App`).
//! - Overlay open/close handling (handled by app event loop).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - App tracks phase configuration and execution state.

use super::super::App;
use crate::progress::ExecutionPhase;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Draw the workflow flowchart overlay.
///
/// Shows a visual representation of the 3-phase workflow with current position.
pub fn draw_flowchart_overlay(f: &mut Frame<'_>, app: &App, area: Rect) {
    // Calculate popup dimensions
    let popup_width = 80.min(area.width.saturating_sub(4)).max(50);
    let popup_height = 20.min(area.height.saturating_sub(4)).max(12);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let title = Line::from(vec![Span::styled(
        "Workflow Flowchart",
        Style::default().add_modifier(Modifier::BOLD),
    )]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

    // Determine if we should use horizontal or vertical layout
    let use_horizontal = inner.width >= 60;

    if use_horizontal {
        draw_flowchart_horizontal(f, app, inner);
    } else {
        draw_flowchart_vertical(f, app, inner);
    }
}

/// Draw horizontal flowchart layout for wide terminals.
fn draw_flowchart_horizontal(f: &mut Frame<'_>, app: &App, area: Rect) {
    let phases: Vec<(&str, ExecutionPhase, &str)> = match app.configured_phases {
        1 => vec![("Single Phase", ExecutionPhase::Planning, "Execute task")],
        2 => vec![
            ("Planning", ExecutionPhase::Planning, "Analyze and plan"),
            (
                "Implementation",
                ExecutionPhase::Implementation,
                "Code and validate",
            ),
        ],
        _ => vec![
            ("Planning", ExecutionPhase::Planning, "Analyze requirements"),
            (
                "Implementation",
                ExecutionPhase::Implementation,
                "Code + CI validation",
            ),
            ("Review", ExecutionPhase::Review, "Review and complete"),
        ],
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(4)].as_ref())
        .split(area);

    let flowchart_area = layout[0];
    let desc_area = layout[1];

    // Build flowchart line
    let mut spans = vec![];
    let phase_count = phases.len();

    for (i, (name, phase, _)) in phases.iter().enumerate() {
        let (icon, style) = get_phase_style(app, *phase);
        let elapsed = app.phase_elapsed(*phase);
        let time_str = if elapsed > std::time::Duration::ZERO {
            format!(" ({})", App::format_duration(elapsed))
        } else {
            String::new()
        };

        // Create phase box content
        let phase_text = format!("{} {}{}", icon, name, time_str);
        let phase_span = Span::styled(phase_text, style);
        spans.push(phase_span);

        // Add arrow between phases
        if i < phase_count.saturating_sub(1) {
            spans.push(Span::styled(
                " -> ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    let flowchart_line = Line::from(spans);
    let paragraph = Paragraph::new(flowchart_line).alignment(Alignment::Center);
    f.render_widget(paragraph, flowchart_area);

    // Draw phase descriptions
    let mut desc_lines = vec![];
    for (name, phase, desc) in phases {
        let (icon, style) = get_phase_style(app, phase);
        desc_lines.push(Line::from(vec![
            Span::styled(format!("{} ", icon), style),
            Span::styled(format!("{}: ", name), style.add_modifier(Modifier::BOLD)),
            Span::styled(desc.to_string(), Style::default().fg(Color::Gray)),
        ]));
    }

    let desc_paragraph = Paragraph::new(Text::from(desc_lines));
    f.render_widget(desc_paragraph, desc_area);
}

/// Draw vertical flowchart layout for narrow terminals.
fn draw_flowchart_vertical(f: &mut Frame<'_>, app: &App, area: Rect) {
    let phases: Vec<(&str, ExecutionPhase, &str)> = match app.configured_phases {
        1 => vec![("Single Phase", ExecutionPhase::Planning, "Execute task")],
        2 => vec![
            ("Planning", ExecutionPhase::Planning, "Analyze and plan"),
            (
                "Implementation",
                ExecutionPhase::Implementation,
                "Code and validate",
            ),
        ],
        _ => vec![
            ("Planning", ExecutionPhase::Planning, "Analyze requirements"),
            (
                "Implementation",
                ExecutionPhase::Implementation,
                "Code + CI validation",
            ),
            ("Review", ExecutionPhase::Review, "Review and complete"),
        ],
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)].as_ref())
        .split(area);

    let flowchart_area = layout[0];
    let hint_area = layout[1];

    // Build vertical flowchart
    let mut lines = vec![];
    let last_idx = phases.len().saturating_sub(1);

    for (i, (name, phase, desc)) in phases.iter().enumerate() {
        let (icon, style) = get_phase_style(app, *phase);
        let elapsed = app.phase_elapsed(*phase);
        let time_str = if elapsed > std::time::Duration::ZERO {
            format!(" ({})", App::format_duration(elapsed))
        } else {
            String::new()
        };

        // Phase line
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", icon), style),
            Span::styled(format!("{}{}", name, time_str), style),
        ]));

        // Description line
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(desc.to_string(), Style::default().fg(Color::DarkGray)),
        ]));

        // Arrow down (except for last phase)
        if i < last_idx {
            lines.push(Line::from(vec![Span::styled(
                "  v",
                Style::default().fg(Color::DarkGray),
            )]));
        }
    }

    let paragraph = Paragraph::new(Text::from(lines));
    f.render_widget(paragraph, flowchart_area);

    // Hint line
    let hint = Line::from(vec![
        Span::styled("Press ", Style::default().fg(Color::DarkGray)),
        Span::styled("f", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" to close", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), hint_area);
}

/// Get the style and icon for a phase based on its status.
fn get_phase_style(app: &App, phase: ExecutionPhase) -> (&'static str, Style) {
    if app.is_phase_active(phase) {
        (
            ">",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else if app.is_phase_completed(phase) {
        ("+", Style::default().fg(Color::Green))
    } else {
        ("o", Style::default().fg(Color::DarkGray))
    }
}
