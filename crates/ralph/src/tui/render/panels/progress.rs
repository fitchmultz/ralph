//! Progress panel rendering for execution view.
//!
//! Responsibilities:
//! - Render phase indicators (Planning, Implementation, Review).
//! - Display timing information and current operation description.
//! - Render progress bar showing completion percentage.
//! - Display ETA estimate based on historical data.
//!
//! Not handled here:
//! - Log rendering or ANSI processing.
//! - Phase state management (reads from `app`).
//! - ETA calculation (receives pre-calculated values).
//!
//! Invariants/assumptions:
//! - Called from execution view when `app.show_progress_panel` is true.
//! - `app.configured_phases` determines which phases to display.
//! - Progress bar width is fixed at 20 characters for consistent layout.

use super::super::App;
use crate::eta_calculator::format_eta;
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

    // Split inner area vertically for phases, progress bar, and operation description
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Phase indicators
            Constraint::Length(1), // Progress bar + percentage
            Constraint::Length(1), // Operation + ETA
        ])
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

    // Draw progress bar with percentage and ETA
    draw_progress_bar(f, app, chunks[1]);

    // Show current operation description with ETA on the third line
    draw_operation_line(f, app, chunks[2]);
}

/// Draw the progress bar showing completion percentage.
fn draw_progress_bar(f: &mut Frame<'_>, app: &App, area: Rect) {
    let percentage = app.completion_percentage();
    let progress_bar = render_progress_bar(percentage, 20);

    let spans = vec![
        Span::raw("["),
        Span::styled(progress_bar.filled, Style::default().fg(Color::Cyan)),
        Span::styled(progress_bar.empty, Style::default().fg(Color::DarkGray)),
        Span::raw("] "),
        Span::styled(
            format!("{:3}%", percentage),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

/// Render a progress bar string for the given percentage.
fn render_progress_bar(percentage: u8, width: usize) -> ProgressBar {
    let filled_width = (percentage as usize * width) / 100;
    let empty_width = width.saturating_sub(filled_width);

    let filled = "█".repeat(filled_width);
    let empty = "░".repeat(empty_width);

    ProgressBar { filled, empty }
}

/// Progress bar string components.
struct ProgressBar {
    filled: String,
    empty: String,
}

/// Draw the operation line with ETA information.
fn draw_operation_line(f: &mut Frame<'_>, app: &App, area: Rect) {
    let operation = app.operation();
    if operation.is_empty() || !app.runner_active {
        return;
    }

    let mut spans = vec![
        Span::styled("Operation: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            operation.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    // Add ETA if available
    if let Some(ref eta) = app.current_eta {
        let eta_str = format_eta(eta.remaining);
        let confidence_color = match eta.confidence {
            crate::eta_calculator::EtaConfidence::High => Color::Green,
            crate::eta_calculator::EtaConfidence::Medium => Color::Yellow,
            crate::eta_calculator::EtaConfidence::Low => Color::Gray,
        };

        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            format!("ETA: {} ", eta_str),
            Style::default().fg(Color::Cyan),
        ));
        spans.push(Span::styled(
            eta.confidence.indicator(),
            Style::default().fg(confidence_color),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}
