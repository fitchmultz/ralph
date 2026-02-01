//! Dependency graph overlay rendering.
//!
//! Responsibilities:
//! - Render dependency graph visualization for the selected task.
//! - Show upstream dependencies (depends_on) or downstream dependents (blocked_by).
//! - Highlight critical path tasks when enabled.
//!
//! Not handled here:
//! - Graph data structure construction (handled by `crate::queue::graph`).
//! - Task selection state (handled by `App`).
//! - Input handling for toggling views (handled by app event loop).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - Graph is built from current queue and done task lists.

use super::super::App;
use crate::contracts::TaskStatus;
use crate::queue::graph;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Draw the dependency graph overlay.
///
/// Shows a visual representation of task dependencies for the selected task.
pub fn draw_dependency_graph_overlay(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    show_dependents: bool,
    highlight_critical: bool,
) {
    // Calculate popup dimensions
    let popup_width = 90.min(area.width.saturating_sub(4)).max(60);
    let popup_height = 25.min(area.height.saturating_sub(4)).max(15);

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let title_text = if show_dependents {
        "Dependency Graph: Blocked By (Downstream)"
    } else {
        "Dependency Graph: Depends On (Upstream)"
    };

    let title = Line::from(vec![Span::styled(
        title_text,
        Style::default().add_modifier(Modifier::BOLD),
    )]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Magenta));
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

    // Get the selected task
    let Some(task) = app.selected_task() else {
        let msg = Paragraph::new("No task selected");
        f.render_widget(msg, inner);
        return;
    };

    // Build dependency graph
    let graph = graph::build_graph(&app.queue, Some(&app.done));

    // Get the tasks to display
    let related_tasks: Vec<_> = if show_dependents {
        graph.get_blocked_chain(&task.id)
    } else {
        graph.get_blocking_chain(&task.id)
    };

    // Calculate critical paths if highlighting is enabled
    let critical_paths = if highlight_critical {
        graph::find_critical_paths(&graph)
    } else {
        Vec::new()
    };

    // Create layout
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)].as_ref())
        .split(inner);

    let content_area = layout[0];
    let hint_area = layout[1];

    // Build content lines
    let mut lines = Vec::new();

    // Header line with current task
    let is_critical = graph.is_on_critical_path(&task.id, &critical_paths);
    let critical_marker = if is_critical { " *" } else { "" };
    let status_emoji = task_status_emoji(task.status);
    lines.push(Line::from(vec![
        Span::styled(
            format!("Current: {}{}", task.id, critical_marker),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("[{}]", status_emoji),
            Style::default().fg(status_color(task.status)),
        ),
        Span::raw(format!(" {}", task.title)),
    ]));

    lines.push(Line::from(""));

    // Related tasks
    if related_tasks.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            if show_dependents {
                "No tasks are blocked by this task"
            } else {
                "This task has no dependencies"
            },
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            if show_dependents {
                "Tasks blocked by this task:"
            } else {
                "Tasks this depends on:"
            },
            Style::default().fg(Color::DarkGray),
        )]));

        for (i, related_id) in related_tasks.iter().take(10).enumerate() {
            if let Some(node) = graph.get(related_id) {
                let is_rel_critical =
                    highlight_critical && graph.is_on_critical_path(related_id, &critical_paths);
                let rel_critical_marker = if is_rel_critical { " *" } else { "" };
                let rel_status_emoji = task_status_emoji(node.task.status);
                let indent = "  ".repeat(i + 1);

                lines.push(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        format!("{}{}", related_id, rel_critical_marker),
                        if is_rel_critical {
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("[{}]", rel_status_emoji),
                        Style::default().fg(status_color(node.task.status)),
                    ),
                    Span::raw(format!(" {}", node.task.title)),
                ]));
            }
        }

        if related_tasks.len() > 10 {
            lines.push(Line::from(vec![Span::styled(
                format!("  ... and {} more", related_tasks.len() - 10),
                Style::default().fg(Color::DarkGray),
            )]));
        }
    }

    // Add critical path info if highlighting is on
    if highlight_critical && !critical_paths.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("* ", Style::default().fg(Color::Red)),
            Span::styled(
                format!("= on critical path (length: {})", critical_paths[0].length),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let paragraph = Paragraph::new(ratatui::text::Text::from(lines));
    f.render_widget(paragraph, content_area);

    // Hint line
    let hint_spans = vec![
        Span::styled("Press ", Style::default().fg(Color::DarkGray)),
        Span::styled("t/Tab", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" to toggle view, ", Style::default().fg(Color::DarkGray)),
        Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" for critical path, ", Style::default().fg(Color::DarkGray)),
        Span::styled("d/Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" to close", Style::default().fg(Color::DarkGray)),
    ];

    let hint = Line::from(hint_spans);
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), hint_area);
}

/// Get emoji for task status
fn task_status_emoji(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "⏳",
        TaskStatus::Doing => "🔄",
        TaskStatus::Done => "✅",
        TaskStatus::Rejected => "❌",
        TaskStatus::Draft => "📝",
    }
}

/// Get color for task status
fn status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Todo => Color::Gray,
        TaskStatus::Doing => Color::Yellow,
        TaskStatus::Done => Color::Green,
        TaskStatus::Rejected => Color::DarkGray,
        TaskStatus::Draft => Color::Blue,
    }
}
