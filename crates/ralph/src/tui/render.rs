//! TUI rendering implementation extracted from `crate::tui`.
//!
//! This module contains all rendering/layout logic for the terminal UI,
//! separated from application state and event handling to keep `tui.rs`
//! focused on interaction and orchestration.
//!
//! Public API is preserved via `crate::tui::draw_ui` re-exporting
//! `render::draw_ui`.

use super::{App, AppMode};
use crate::contracts::{TaskPriority, TaskStatus};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Draw the main UI.
///
/// Public to allow testing with TestBackend.
/// Re-exported from `crate::tui` as `tui::draw_ui`.
pub fn draw_ui(f: &mut Frame<'_>, app: &mut App) {
    let size = f.area();

    // Handle Executing mode separately (full-screen output view)
    if matches!(app.mode, AppMode::Executing { .. }) {
        draw_execution_view(f, app, size);
        return;
    }

    // Main layout: split into left (task list) and right (details)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(size);

    // Left panel: task list
    draw_task_list(f, app, chunks[0]);

    // Right panel: task details
    draw_task_details(f, app, chunks[1]);

    // Draw confirmation dialog if in ConfirmDelete mode
    if app.mode == AppMode::ConfirmDelete {
        draw_confirm_dialog(f, size);
    }
}

/// Wrap text to fit within a given width.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    textwrap::wrap(text, width)
        .into_iter()
        .map(|s| s.into_owned())
        .collect()
}

/// Draw the execution view (full-screen output during task execution).
fn draw_execution_view(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let task_id = match &app.mode {
        AppMode::Executing { task_id } => task_id.clone(),
        _ => "Unknown".to_string(),
    };

    // Create a block with title
    let title = Line::from(vec![
        Span::styled("Executing: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(&task_id, Style::default().fg(Color::Cyan)),
        Span::raw(" "),
        Span::styled("(Esc to return)", Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Calculate visible log lines
    let visible_height = inner.height.saturating_sub(2) as usize; // Leave room for borders
    let log_count = app.logs.len();
    let start_idx = if app.log_scroll + visible_height > log_count {
        log_count.saturating_sub(visible_height)
    } else {
        app.log_scroll
    };

    // Get visible log lines
    let visible_logs: Vec<&String> = app
        .logs
        .iter()
        .skip(start_idx)
        .take(visible_height)
        .collect();

    // Render logs
    let log_text = Text::from(
        visible_logs
            .iter()
            .map(|line| Line::from(line.as_str()))
            .collect::<Vec<_>>(),
    );

    let paragraph = Paragraph::new(log_text)
        .block(Block::default())
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, inner);

    // Draw status indicator at bottom
    let status_line = if log_count > 0 {
        Line::from(vec![
            Span::raw("Lines: "),
            Span::styled(format!("{}", log_count), Style::default().fg(Color::Cyan)),
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
        ])
    } else {
        Line::from(vec![Span::styled(
            "Waiting for output...",
            Style::default().fg(Color::DarkGray),
        )])
    };

    let status_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };

    let status_paragraph = Paragraph::new(status_line);
    f.render_widget(status_paragraph, status_area);
}

/// Draw the task list panel.
fn draw_task_list(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let title = Line::from(vec![
        Span::styled("Tasks", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" ("),
        Span::styled(
            format!("{}", app.queue.tasks.len()),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(")"),
    ]);

    let list_height = area.height.saturating_sub(2) as usize; // Subtract borders
    app.list_height = list_height;

    let items: Vec<ListItem> = app
        .queue
        .tasks
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(list_height)
        .map(|(i, task)| {
            let is_selected = i == app.selected;
            let status_style = Style::default().fg(status_color(task.status));

            let line = if is_selected {
                Line::from(vec![
                    Span::styled(
                        "» ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&task.id, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" "),
                    Span::styled(
                        task.status.as_str(),
                        status_style.add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(task.priority.as_str(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(&task.title, Style::default().add_modifier(Modifier::BOLD)),
                ])
            } else {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(&task.id, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(task.status.as_str(), status_style),
                    Span::raw(" "),
                    Span::styled(task.priority.as_str(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(&task.title, Style::default()),
                ])
            };

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));

    f.render_widget(list, area);

    // Draw selection indicator manually
    if !app.queue.tasks.is_empty() {
        let list_height = area.height.saturating_sub(2) as usize; // Subtract borders
        let visible_count = list_height.min(app.queue.tasks.len());
        let selected_offset = app.selected.saturating_sub(app.scroll);

        if selected_offset < visible_count {
            let y = area.y + 1 + selected_offset as u16;
            let highlight_area = Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            };
            f.render_widget(
                Paragraph::new("").block(Block::default().style(Style::default().bg(Color::Blue))),
                highlight_area,
            );
        }
    }
}

/// Draw the task details panel.
fn draw_task_details(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    app.detail_width = area.width.saturating_sub(4); // Account for borders

    let title = if let AppMode::EditingTitle(ref title) = &app.mode {
        Line::from(vec![
            Span::styled(
                "Edit Title: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Yellow)), // Cursor
        ])
    } else {
        Line::from(Span::styled(
            "Task Details",
            Style::default().add_modifier(Modifier::BOLD),
        ))
    };

    let block = Block::default().title(title).borders(Borders::ALL);
    f.render_widget(block, area);

    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    if let Some(task) = app.selected_task() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID:       ", Style::default().fg(Color::DarkGray)),
                Span::styled(&task.id, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Status:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    task.status.as_str(),
                    Style::default().fg(status_color(task.status)),
                ),
            ]),
            Line::from(vec![
                Span::styled("Priority: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    task.priority.as_str(),
                    Style::default().fg(priority_color(task.priority)),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Title", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]),
        ];

        // Title with word wrap
        for line in wrap_text(&task.title, app.detail_width as usize) {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().add_modifier(Modifier::BOLD),
            )));
        }

        // Tags
        if !task.tags.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Tags", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(": ", Style::default()),
                Span::styled(task.tags.join(", "), Style::default().fg(Color::Cyan)),
            ]));
        }

        // Scope
        if !task.scope.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Scope", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(": ", Style::default()),
                Span::styled(task.scope.join(", "), Style::default().fg(Color::Green)),
            ]));
        }

        // Evidence
        if !task.evidence.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Evidence",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(":", Style::default()),
            ]));
            for item in &task.evidence {
                for line in wrap_text(item, app.detail_width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(
                        format!("  • {}", line),
                        Style::default(),
                    )));
                }
            }
        }

        // Plan
        if !task.plan.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Plan", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]));
            for (i, item) in task.plan.iter().enumerate() {
                for line in wrap_text(item, app.detail_width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(
                        format!("  {}. {}", i + 1, line),
                        Style::default(),
                    )));
                }
            }
        }

        // Notes
        if !task.notes.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Notes", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]));
            for item in &task.notes {
                for line in wrap_text(item, app.detail_width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(
                        format!("  - {}", line),
                        Style::default().fg(Color::Yellow),
                    )));
                }
            }
        }

        // Dependencies
        if !task.depends_on.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Depends On",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(": ", Style::default()),
                Span::styled(
                    task.depends_on.join(", "),
                    Style::default().fg(Color::Magenta),
                ),
            ]));
        }

        // Custom Fields
        if !task.custom_fields.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Custom Fields",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(":", Style::default()),
            ]));
            let mut sorted_fields: Vec<_> = task.custom_fields.iter().collect();
            sorted_fields.sort_by_key(|&(k, _)| k);
            for (key, value) in sorted_fields {
                for line in wrap_text(
                    &format!("  • {}: {}", key, value),
                    app.detail_width.saturating_sub(4) as usize,
                ) {
                    lines.push(Line::from(Span::styled(
                        line,
                        Style::default().fg(Color::LightCyan),
                    )));
                }
            }
        }

        // Timestamps
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "Created",
                Style::default().add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled(": ", Style::default()),
            Span::styled(
                task.created_at.as_deref().unwrap_or("N/A"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Updated",
                Style::default().add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled(": ", Style::default()),
            Span::styled(
                task.updated_at.as_deref().unwrap_or("N/A"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    } else {
        let text = Text::from(vec![
            Line::from(""),
            Line::from("No tasks in queue."),
            Line::from(""),
            Line::from("Create a task with:"),
            Line::from(Span::styled(
                "  ralph task \"your request\"",
                Style::default().fg(Color::Cyan),
            )),
        ]);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }

    // Draw help footer at bottom of screen
    let help_text = match &app.mode {
        AppMode::Normal => vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":quit "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":nav "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":run "),
            Span::styled("d", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":del "),
            Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":edit "),
            Span::styled("s", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":status"),
        ],
        AppMode::EditingTitle(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":save "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::ConfirmDelete => vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":yes "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":no "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::Executing { .. } => vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":return to list (task continues)"),
        ],
    };

    let help_paragraph = Paragraph::new(Line::from(help_text))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    let help_area = Rect {
        x: 0,
        y: f.area().height.saturating_sub(1),
        width: f.area().width,
        height: 1,
    };
    f.render_widget(help_paragraph, help_area);
}

/// Draw the confirmation dialog for task deletion.
fn draw_confirm_dialog(f: &mut Frame<'_>, area: Rect) {
    let popup_width = 40.min(area.width.saturating_sub(4));
    let popup_height = 6;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let popup = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Delete this task? ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("(y/n)", Style::default().fg(Color::Yellow)),
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

/// Get the color for a task status.
fn status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Draft => Color::DarkGray,
        TaskStatus::Todo => Color::Blue,
        TaskStatus::Doing => Color::Yellow,
        TaskStatus::Done => Color::Green,
        TaskStatus::Rejected => Color::Red,
    }
}

/// Get the color for a task priority.
fn priority_color(priority: TaskPriority) -> Color {
    match priority {
        TaskPriority::Critical => Color::Red,
        TaskPriority::High => Color::Yellow,
        TaskPriority::Medium => Color::Blue,
        TaskPriority::Low => Color::DarkGray,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_returns_nonempty_for_nonempty_input() {
        let lines = wrap_text("hello world", 5);
        assert!(!lines.is_empty());
        assert!(lines
            .iter()
            .any(|l| l.contains("hello") || l.contains("world")));
    }

    #[test]
    fn status_color_maps_all_statuses() {
        assert_eq!(status_color(TaskStatus::Draft), Color::DarkGray);
        assert_eq!(status_color(TaskStatus::Todo), Color::Blue);
        assert_eq!(status_color(TaskStatus::Doing), Color::Yellow);
        assert_eq!(status_color(TaskStatus::Done), Color::Green);
        assert_eq!(status_color(TaskStatus::Rejected), Color::Red);
    }

    #[test]
    fn priority_color_maps_all_priorities() {
        assert_eq!(priority_color(TaskPriority::Critical), Color::Red);
        assert_eq!(priority_color(TaskPriority::High), Color::Yellow);
        assert_eq!(priority_color(TaskPriority::Medium), Color::Blue);
        assert_eq!(priority_color(TaskPriority::Low), Color::DarkGray);
    }
}
