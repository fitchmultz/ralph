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
//! - This function may mutate the graph cache in `app` for performance.

use super::super::App;
use crate::contracts::TaskStatus;

/// Data for a related task in the dependency graph overlay.
///
/// Pre-computed task information to avoid borrow checker issues
/// when rendering after collecting from the graph.
#[derive(Debug, Clone)]
struct RelatedTaskInfo {
    id: String,
    status: TaskStatus,
    title: String,
    is_critical: bool,
}

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Maximum number of related tasks to display in the overlay.
///
/// This limits both the visual display and the traversal depth for performance.
const DISPLAY_LIMIT: usize = 10;

/// Draw the dependency graph overlay.
///
/// Shows a visual representation of task dependencies for the selected task.
/// Uses cached graph and critical paths when available, and limits chain
/// traversal for performance with large dependency graphs.
pub fn draw_dependency_graph_overlay(
    f: &mut Frame<'_>,
    app: &mut App,
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

    // Copy task fields we need to avoid borrow conflicts with cache
    let task_id = task.id.clone();
    let task_status = task.status;
    let task_title = task.title.clone();

    // Get revisions once to avoid multiple calls
    let queue_rev = app.queue_rev();
    let done_rev = app.done_rev();

    // First, get critical paths if highlighting (this builds graph if needed)
    let critical_paths: Vec<_> = if highlight_critical {
        app.dependency_graph_cache
            .critical_paths(queue_rev, done_rev, &app.queue, &app.done)
            .to_vec()
    } else {
        Vec::new()
    };

    // Now get graph and extract all data we need in one scope
    let (bounded_result, is_critical, related_task_data, critical_length) = {
        let graph = app
            .dependency_graph_cache
            .graph(queue_rev, done_rev, &app.queue, &app.done);

        // Get the tasks to display using bounded traversal for performance
        let bounded_result = if show_dependents {
            graph.get_blocked_chain_bounded(&task_id, DISPLAY_LIMIT)
        } else {
            graph.get_blocking_chain_bounded(&task_id, DISPLAY_LIMIT)
        };

        // Compute if current task is on critical path
        let is_critical = graph.is_on_critical_path(&task_id, &critical_paths);

        // Collect data for related tasks while we have graph borrow
        let related_task_data: Vec<_> = bounded_result
            .task_ids
            .iter()
            .map(|related_id| {
                if let Some(node) = graph.get(related_id) {
                    RelatedTaskInfo {
                        id: related_id.clone(),
                        status: node.task.status,
                        title: node.task.title.clone(),
                        is_critical: highlight_critical
                            && graph.is_on_critical_path(related_id, &critical_paths),
                    }
                } else {
                    RelatedTaskInfo {
                        id: related_id.clone(),
                        status: TaskStatus::Todo,
                        title: "Unknown".to_string(),
                        is_critical: false,
                    }
                }
            })
            .collect();

        let critical_length = if highlight_critical && !critical_paths.is_empty() {
            Some(critical_paths[0].length)
        } else {
            None
        };

        (
            bounded_result,
            is_critical,
            related_task_data,
            critical_length,
        )
    };
    // graph borrow is now dropped

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
    let critical_marker = if is_critical { " *" } else { "" };
    let status_emoji = task_status_emoji(task_status);
    lines.push(Line::from(vec![
        Span::styled(
            format!("Current: {}{}", task_id, critical_marker),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("[{}]", status_emoji),
            Style::default().fg(status_color(task_status)),
        ),
        Span::raw(format!(" {}", task_title)),
    ]));

    lines.push(Line::from(""));

    // Related tasks
    if related_task_data.is_empty() {
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

        for (i, task_info) in related_task_data.iter().enumerate() {
            let rel_critical_marker = if task_info.is_critical { " *" } else { "" };
            let rel_status_emoji = task_status_emoji(task_info.status);
            let indent = "  ".repeat(i + 1);

            lines.push(Line::from(vec![
                Span::raw(indent),
                Span::styled(
                    format!("{}{}", task_info.id, rel_critical_marker),
                    if task_info.is_critical {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", rel_status_emoji),
                    Style::default().fg(status_color(task_info.status)),
                ),
                Span::raw(format!(" {}", task_info.title)),
            ]));
        }

        // Show truncation indicator without count (avoiding full traversal)
        if bounded_result.truncated {
            lines.push(Line::from(vec![Span::styled(
                "  ... and more (truncated)",
                Style::default().fg(Color::DarkGray),
            )]));
        }
    }

    // Add critical path info if highlighting is on
    if let Some(length) = critical_length {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("* ", Style::default().fg(Color::Red)),
            Span::styled(
                format!("= on critical path (length: {})", length),
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
