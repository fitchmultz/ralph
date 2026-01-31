//! Progress panel rendering for execution view.
//!
//! Responsibilities:
//! - Render phase indicators (Planning, Implementation, Review).
//! - Display timing information and current operation description.
//!
//! Not handled here:
//! - Log rendering or ANSI processing.
//! - Phase state management (reads from `app`).
//!
//! Invariants/assumptions:
//! - Called from execution view when `app.show_progress_panel` is true.
//! - `app.configured_phases` determines which phases to display.

use super::super::App;
use crate::progress::ExecutionPhase;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::time::Duration;

/// Draw the progress panel showing phase indicators, timing, and operation description.
pub fn draw_progress_panel(f: &mut Frame<'_>, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Progress")
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split inner area vertically for phases and operation description
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    // Create phase indicators based on configured phases
    let phases: Vec<(&str, ExecutionPhase)> = match app.configured_phases {
        1 => vec![("Single Phase", ExecutionPhase::Planning)],
        2 => vec![
            ("Planning", ExecutionPhase::Planning),
            ("Implementation", ExecutionPhase::Implementation),
        ],
        _ => vec![
            ("Planning", ExecutionPhase::Planning),
            ("Implementation", ExecutionPhase::Implementation),
            ("Review", ExecutionPhase::Review),
        ],
    };

    let mut spans = vec![Span::raw(" ")];

    for (i, (name, phase)) in phases.iter().enumerate() {
        let (icon, style) = if app.is_phase_active(*phase) {
            // Active phase: animated spinner with yellow styling
            let spinner_frame = app.spinner_frame();
            (
                spinner_frame,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        } else if app.is_phase_completed(*phase) {
            // Completed: green checkmark
            ("✓", Style::default().fg(Color::Green))
        } else {
            // Pending: gray circle
            ("○", Style::default().fg(Color::DarkGray))
        };

        let elapsed = app.phase_elapsed(*phase);
        let time_str = if elapsed > Duration::ZERO {
            format!(" {}", App::format_duration(elapsed))
        } else {
            String::new()
        };

        spans.push(Span::styled(
            format!("{} {}{}", icon, name, time_str),
            style,
        ));

        // Add separator between phases
        if i < phases.len() - 1 {
            spans.push(Span::styled(" → ", Style::default().fg(Color::DarkGray)));
        }
    }

    // Add total time
    let total = app.total_execution_time();
    if total > Duration::ZERO {
        spans.push(Span::styled(
            format!(" | Total: {}", App::format_duration(total)),
            Style::default().fg(Color::Cyan),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    f.render_widget(paragraph, chunks[0]);

    // Show current operation description on the second line
    let operation = app.operation();
    if !operation.is_empty() && app.runner_active {
        let operation_line = Line::from(vec![
            Span::styled("Operation: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                operation.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        let operation_paragraph = Paragraph::new(operation_line).alignment(Alignment::Center);
        f.render_widget(operation_paragraph, chunks[1]);
    }
}
