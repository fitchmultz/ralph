//! Execution view panel rendering.
//!
//! Responsibilities:
//! - Render the full-screen execution view during task runs.
//! - Display ANSI-aware terminal output using tui-term.
//! - Show progress panel, log area, and status bar.
//!
//! Not handled here:
//! - Task list or details rendering.
//! - Log buffer management (reads from `app.log_ansi_buffer`).
//!
//! Invariants/assumptions:
//! - `app.running_task_id` is set when in Executing mode.
//! - `app.log_ansi_buffer` contains valid ANSI sequences.

use super::super::App;
use super::progress::draw_progress_panel;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use tui_term::widget::PseudoTerminal;

/// Draw the execution view (full-screen output during task execution).
pub fn draw_execution_view(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    app.clear_list_area();
    let task_id = app.running_task_id.as_deref().unwrap_or("Unknown");

    // Create a block with title
    let mut title_spans = vec![
        Span::styled("Executing: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(task_id, Style::default().fg(Color::Cyan)),
        Span::raw(" "),
        Span::styled("(Esc to return)", Style::default().fg(Color::DarkGray)),
    ];

    if app.loop_active {
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled(
            format!("[Loop: ON, ran {}]", app.loop_ran),
            Style::default().fg(Color::Yellow),
        ));
    }

    let title = Line::from(title_spans);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split area: progress panel (optional) + logs + status bar
    let main_chunks = if app.show_progress_panel && app.runner_active {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Progress panel height
                Constraint::Min(1),    // Log area
                Constraint::Length(1), // Status bar
            ])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Log area (full height)
                Constraint::Length(1), // Status bar
            ])
            .split(inner)
    };

    let progress_area = if app.show_progress_panel && app.runner_active {
        Some(main_chunks[0])
    } else {
        None
    };
    let log_idx = if app.show_progress_panel && app.runner_active {
        1
    } else {
        0
    };
    let status_idx = if app.show_progress_panel && app.runner_active {
        2
    } else {
        1
    };

    // Render progress panel if visible
    if let Some(area) = progress_area {
        draw_progress_panel(f, app, area);
    }

    let log_area = main_chunks[log_idx];
    let status_area = main_chunks[status_idx];

    // Clear areas first to prevent artifacts from previous renders
    f.render_widget(Clear, log_area);
    f.render_widget(Clear, status_area);

    // Calculate visible log lines for scroll tracking
    let visible_height = log_area.height as usize;
    app.set_log_visible_lines(visible_height);
    let log_count = app.logs.len();

    // Render logs using tui-term's PseudoTerminal for ANSI-aware display
    // Handle edge cases for very small terminal sizes
    if log_area.width == 0 || log_area.height == 0 {
        // Terminal too small, render nothing in log area
    } else if !app.log_ansi_buffer.is_empty() {
        // Create vt100 parser with current dimensions
        // Note: vt100 parser uses (rows, cols) order
        let mut parser = vt100::Parser::new(log_area.height, log_area.width, 0);
        parser.process(&app.log_ansi_buffer);

        // Render using PseudoTerminal widget
        let pseudo_term = PseudoTerminal::new(parser.screen()).block(Block::default());
        f.render_widget(pseudo_term, log_area);
    } else {
        // No ANSI buffer yet, render a placeholder
        let placeholder = Paragraph::new("Waiting for output...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(placeholder, log_area);
    }

    // Draw status indicator at bottom
    let mut status_parts = vec![
        Span::raw("Lines: "),
        Span::styled(log_count.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(" | Scroll: "),
        Span::styled(
            format!("{}/{}", app.log_scroll, log_count),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" | "),
        Span::styled("Auto-scroll: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if app.autoscroll { "ON" } else { "OFF" },
            Style::default().fg(if app.autoscroll {
                Color::Green
            } else {
                Color::Red
            }),
        ),
    ];

    if app.loop_active {
        status_parts.push(Span::raw(" | "));
        status_parts.push(Span::styled(
            format!("Loop ran {}", app.loop_ran),
            Style::default().fg(Color::Yellow),
        ));
        if let Some(max) = app.loop_max_tasks {
            status_parts.push(Span::raw(" / "));
            status_parts.push(Span::styled(
                format!("{}", max),
                Style::default().fg(Color::Yellow),
            ));
        }
    }

    let status_line = if log_count > 0 {
        Line::from(status_parts)
    } else {
        Line::from(vec![Span::styled(
            "Waiting for output...",
            Style::default().fg(Color::DarkGray),
        )])
    };

    let status_paragraph = Paragraph::new(status_line);
    f.render_widget(status_paragraph, status_area);
}
