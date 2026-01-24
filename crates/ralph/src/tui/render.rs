//! TUI rendering implementation extracted from `crate::tui`.
//!
//! This module contains all rendering/layout logic for the terminal UI,
//! separated from application state and event handling to keep `tui.rs`
//! focused on interaction and orchestration.
//!
//! Public API is preserved via `crate::tui::draw_ui` re-exporting
//! `render::draw_ui`.

use super::{App, AppMode, ConfigFieldKind, TaskEditKind};
use crate::contracts::{TaskPriority, TaskStatus};
use crate::outpututil::truncate_chars;
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

    // Handle Executing mode (full-screen output view), including modal prompts layered on top.
    let show_execution = match app.mode.clone() {
        AppMode::Executing { .. } => true,
        AppMode::ConfirmRevert { previous_mode, .. } => {
            matches!(*previous_mode, AppMode::Executing { .. })
        }
        _ => false,
    };

    if show_execution {
        draw_execution_view(f, app, size);
    } else {
        // Reserve a footer row for help + status.
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(2), Constraint::Length(1)].as_ref())
            .split(size);
        let main = outer[0];
        let footer = outer[1];

        // Responsive main layout:
        // - If narrow, stack list and details vertically.
        // - If wide, split horizontally.
        let chunks = if main.width < 90 {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
                .split(main)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
                .split(main)
        };

        // Left/top panel: task list
        draw_task_list(f, app, chunks[0]);

        // Right/bottom panel: task details
        draw_task_details(f, app, chunks[1]);

        // Footer (help + status).
        draw_footer(f, app, footer);
    }

    // Help overlay (full-screen).
    if app.mode == AppMode::Help {
        draw_help_overlay(f, size);
    }

    // Confirmation dialogs.
    if app.mode == AppMode::ConfirmDelete {
        draw_confirm_dialog(f, size, "Delete this task?", "(y/n)");
    } else if app.mode == AppMode::ConfirmArchive {
        draw_confirm_dialog(f, size, "Archive done/rejected tasks?", "(y/n)");
    } else if app.mode == AppMode::ConfirmQuit {
        draw_confirm_dialog(f, size, "Task still running. Quit?", "(y/n)");
    } else if let AppMode::ConfirmRevert {
        label,
        selected,
        input,
        ..
    } = &app.mode
    {
        draw_revert_dialog(f, size, label, *selected, input);
    }

    // Command palette overlay.
    if let AppMode::CommandPalette { query, selected } = &app.mode {
        draw_command_palette(f, app, size, query, *selected);
    }

    // Config editor overlay.
    if let AppMode::EditingConfig {
        selected,
        editing_value,
    } = &app.mode
    {
        draw_config_editor(f, app, size, *selected, editing_value.as_deref());
    }

    // Task editor overlay.
    if let AppMode::EditingTask {
        selected,
        editing_value,
    } = &app.mode
    {
        draw_task_editor(f, app, size, *selected, editing_value.as_deref());
    }
}

/// Wrap text to fit within a given width.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    textwrap::wrap(text, width)
        .into_iter()
        .map(|s| s.into_owned())
        .collect()
}

/// Draw the footer area.
fn draw_footer(f: &mut Frame<'_>, app: &App, area: Rect) {
    let help_text = help_footer_spans(app);

    let help_paragraph = Paragraph::new(Line::from(help_text))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    f.render_widget(help_paragraph, area);
}

/// Draw the full-screen help overlay with keybindings.
fn draw_help_overlay(f: &mut Frame<'_>, area: Rect) {
    let popup = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    f.render_widget(Clear, popup);

    let block = Block::default()
        .title(Line::from(Span::styled(
            "Help",
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .borders(Borders::ALL);
    f.render_widget(block, popup);

    let inner = popup.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    let lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "Keybindings",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Press Esc or ?/h to close."),
        Line::from(""),
        Line::from(Span::styled(
            "Navigation",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Up/Down or j/k: move selection"),
        Line::from("Enter: run selected task"),
        Line::from(""),
        Line::from(Span::styled(
            "Actions",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("l: toggle loop mode"),
        Line::from("a: archive done/rejected tasks"),
        Line::from("d: delete selected task"),
        Line::from("e: edit task fields"),
        Line::from("n: create a new task"),
        Line::from("c: edit project config"),
        Line::from("g: scan repository"),
        Line::from("r: reload queue from disk"),
        Line::from("q: quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Filters & Search",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("/: search tasks"),
        Line::from("t: filter by tags"),
        Line::from("o: filter by scope"),
        Line::from("f: cycle status filter"),
        Line::from("x: clear filters"),
        Line::from("C: toggle case-sensitive search"),
        Line::from("R: toggle regex search"),
        Line::from(""),
        Line::from(Span::styled(
            "Quick Changes",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("s: cycle task status"),
        Line::from("p: cycle priority"),
        Line::from(""),
        Line::from(Span::styled(
            "Command Palette",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(": open palette (type to filter, Enter to run, Esc to cancel)"),
        Line::from(""),
        Line::from(Span::styled(
            "Execution View",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Esc: return to task list"),
        Line::from("Up/Down or j/k: scroll logs"),
        Line::from("PgUp/PgDn: page logs"),
        Line::from("a: toggle auto-scroll"),
        Line::from("l: stop loop mode"),
    ];

    let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

/// Draw the execution view (full-screen output during task execution).
fn draw_execution_view(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let task_id = app
        .running_task_id
        .as_deref()
        .unwrap_or("Unknown")
        .to_string();

    // Create a block with title
    let mut title_spans = vec![
        Span::styled("Executing: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(&task_id, Style::default().fg(Color::Cyan)),
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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(inner);
    let log_area = chunks[0];
    let status_area = chunks[1];

    f.render_widget(Clear, log_area);
    f.render_widget(Clear, status_area);

    // Calculate visible log lines
    let visible_height = log_area.height as usize;
    app.set_log_visible_lines(visible_height);
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

    f.render_widget(paragraph, log_area);

    // Draw status indicator at bottom
    let mut status_parts = vec![
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

/// Draw the task list panel.
fn draw_task_list(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let total_count = app.queue.tasks.len();
    let visible_count = app.filtered_len();
    let count_label = if app.has_active_filters() {
        format!("{}/{}", visible_count, total_count)
    } else {
        format!("{}", total_count)
    };
    let filter_summary = app.filter_summary();

    let mut title_spans = vec![
        Span::styled("Tasks", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" ("),
        Span::styled(count_label, Style::default().fg(Color::DarkGray)),
        Span::raw(") "),
        Span::styled(
            filter_summary.unwrap_or_default(),
            Style::default().fg(Color::DarkGray),
        ),
    ];

    if app.runner_active {
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled(
            "RUNNING",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        if let Some(id) = app.running_task_id.as_deref() {
            title_spans.push(Span::raw(" "));
            title_spans.push(Span::styled(
                id.to_string(),
                Style::default().fg(Color::Cyan),
            ));
        }
    }

    if app.loop_active {
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled(
            "LOOP",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled(
            format!("ran {}", app.loop_ran),
            Style::default().fg(Color::Yellow),
        ));
        if let Some(max) = app.loop_max_tasks {
            title_spans.push(Span::raw("/"));
            title_spans.push(Span::styled(
                format!("{}", max),
                Style::default().fg(Color::Yellow),
            ));
        }
    }

    let title = Line::from(title_spans);

    let list_height = area.height.saturating_sub(2) as usize; // Subtract borders
    app.list_height = list_height;

    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(list_height)
        .filter_map(|(i, &task_index)| {
            let task = app.queue.tasks.get(task_index)?;
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
                    Span::raw(" "),
                    Span::styled(&task.id, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(task.status.as_str(), status_style),
                    Span::raw(" "),
                    Span::styled(task.priority.as_str(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(&task.title, Style::default()),
                ])
            };

            Some(ListItem::new(line))
        })
        .collect();

    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));

    f.render_widget(list, area);

    // Draw selection indicator manually
    if app.filtered_len() > 0 {
        let list_height = area.height.saturating_sub(2) as usize; // Subtract borders
        let visible_count = list_height.min(app.filtered_len());
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

    let title = match &app.mode {
        AppMode::EditingTask { .. } => Line::from(Span::styled(
            "Task Editor",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        AppMode::CreatingTask(title) => Line::from(vec![
            Span::styled("New Task: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Yellow)), // Cursor
        ]),
        AppMode::Searching(query) => Line::from(vec![
            Span::styled("Search: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                query,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Yellow)), // Cursor
        ]),
        AppMode::FilteringTags(tags) => Line::from(vec![
            Span::styled(
                "Filter Tags: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                tags,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Yellow)), // Cursor
        ]),
        AppMode::FilteringScopes(scopes) => Line::from(vec![
            Span::styled(
                "Filter Scopes: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                scopes,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Yellow)), // Cursor
        ]),
        AppMode::Scanning(focus) => Line::from(vec![
            Span::styled(
                "Scan Focus: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                focus,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Yellow)), // Cursor
        ]),
        AppMode::CommandPalette { .. } => Line::from(Span::styled(
            "Task Details",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        _ => Line::from(Span::styled(
            "Task Details",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    };

    let block = Block::default().title(title).borders(Borders::ALL);
    f.render_widget(block, area);

    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    if let AppMode::CreatingTask(current) = &app.mode {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled("(auto)", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    TaskStatus::Todo.as_str(),
                    Style::default().fg(status_color(TaskStatus::Todo)),
                ),
            ]),
            Line::from(vec![
                Span::styled("Priority: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    TaskPriority::Medium.as_str(),
                    Style::default().fg(priority_color(TaskPriority::Medium)),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Title", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]),
        ];

        let title_text = if current.is_empty() {
            "(enter a title)"
        } else {
            current
        };
        for line in wrap_text(title_text, app.detail_width as usize) {
            let style = if current.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
        return;
    }

    if let AppMode::Searching(current) = &app.mode {
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "Search Query",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(":", Style::default()),
            ]),
            Line::from(""),
        ];
        let display = if current.is_empty() {
            "(type to search across title, tags, scope, plan, evidence, notes)"
        } else {
            current
        };
        for line in wrap_text(display, app.detail_width as usize) {
            let style = if current.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Enter to apply or Esc to cancel.",
            Style::default().fg(Color::DarkGray),
        )));
        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
        return;
    }

    if let AppMode::FilteringTags(current) = &app.mode {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Tags", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(" (comma-separated):", Style::default()),
            ]),
            Line::from(""),
        ];
        let display = if current.is_empty() {
            "(e.g., tui, ux, docs)"
        } else {
            current
        };
        for line in wrap_text(display, app.detail_width as usize) {
            let style = if current.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Enter to apply or Esc to cancel.",
            Style::default().fg(Color::DarkGray),
        )));
        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
        return;
    }

    if let AppMode::Scanning(current) = &app.mode {
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "Scan Focus",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(":", Style::default()),
            ]),
            Line::from(""),
        ];
        let display = if current.is_empty() {
            "(optional: describe what to scan for)"
        } else {
            current
        };
        for line in wrap_text(display, app.detail_width as usize) {
            let style = if current.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Enter to start scan or Esc to cancel.",
            Style::default().fg(Color::DarkGray),
        )));
        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
        return;
    }

    if let Some(task) = app.selected_task() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&task.id, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
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

        for line in wrap_text(&task.title, app.detail_width as usize) {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().add_modifier(Modifier::BOLD),
            )));
        }

        if !task.tags.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Tags", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(": ", Style::default()),
                Span::styled(task.tags.join(", "), Style::default().fg(Color::Cyan)),
            ]));
        }

        if !task.scope.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Scope", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(": ", Style::default()),
                Span::styled(task.scope.join(", "), Style::default().fg(Color::Green)),
            ]));
        }

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
                        format!(" • {}", line),
                        Style::default(),
                    )));
                }
            }
        }

        if !task.plan.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Plan", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]));
            for (i, item) in task.plan.iter().enumerate() {
                for line in wrap_text(item, app.detail_width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(
                        format!(" {}. {}", i + 1, line),
                        Style::default(),
                    )));
                }
            }
        }

        if !task.notes.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Notes", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]));
            for item in &task.notes {
                for line in wrap_text(item, app.detail_width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(
                        format!(" - {}", line),
                        Style::default().fg(Color::Yellow),
                    )));
                }
            }
        }

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

        if !task.custom_fields.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Custom Fields",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(":", Style::default()),
            ]));
            // Note: We inline custom fields sorting/formatting here (rather than using
            // format_custom_fields from outpututil) because we need per-line wrapping
            // for the TUI display. The format_custom_fields helper returns a single
            // concatenated string which doesn't work for our text layout.
            let mut sorted_fields: Vec<_> = task.custom_fields.iter().collect();
            sorted_fields.sort_by_key(|&(k, _)| k);
            for (key, value) in sorted_fields {
                for line in wrap_text(
                    &format!(" • {}: {}", key, value),
                    app.detail_width.saturating_sub(4) as usize,
                ) {
                    lines.push(Line::from(Span::styled(
                        line,
                        Style::default().fg(Color::LightCyan),
                    )));
                }
            }
        }

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
        return;
    }

    if app.queue.tasks.is_empty() {
        let text = Text::from(vec![
            Line::from(""),
            Line::from("No tasks in queue."),
            Line::from(""),
            Line::from("Create a task with:"),
            Line::from(Span::styled(
                " ralph task \"your request\"",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press n to create one in the TUI.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "Press : to open the command palette.",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
        return;
    }

    let filter_hint = app
        .filter_summary()
        .unwrap_or_else(|| "filters active".to_string());
    let text = Text::from(vec![
        Line::from(""),
        Line::from("No tasks match current filters."),
        Line::from(Span::styled(
            filter_hint,
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press x to clear filters.",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(
            "Press / to search or t to filter tags.",
            Style::default().fg(Color::DarkGray),
        )),
    ]);
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

/// Draw the command palette overlay.
fn draw_command_palette(f: &mut Frame<'_>, app: &App, area: Rect, query: &str, selected: usize) {
    let entries = app.palette_entries(query);

    let popup_width = 70.min(area.width.saturating_sub(4));
    let popup_height = (entries.len() as u16 + 4)
        .min(area.height.saturating_sub(4))
        .max(6);

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
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
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
        .split(inner);

    let input = Line::from(vec![
        Span::styled(
            ":",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(query.to_string(), Style::default().fg(Color::Yellow)),
        Span::styled("_", Style::default().fg(Color::Yellow)),
    ]);
    f.render_widget(Paragraph::new(input), inner_chunks[0]);

    let list_height = inner_chunks[1].height as usize;
    let visible_entries = entries.iter().take(list_height).collect::<Vec<_>>();

    let items: Vec<ListItem> = if visible_entries.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "(no matches)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        visible_entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let is_selected = idx == selected.min(visible_entries.len().saturating_sub(1));
                let style = if is_selected {
                    Style::default()
                        .bg(Color::Blue)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(entry.title.clone(), style)))
            })
            .collect()
    };

    let list = List::new(items).block(Block::default());
    f.render_widget(list, inner_chunks[1]);
}

/// Draw the confirmation dialog for a destructive action.
fn draw_confirm_dialog(f: &mut Frame<'_>, area: Rect, message: &str, hint: &str) {
    let popup_width = 44.min(area.width.saturating_sub(4));
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

fn draw_revert_dialog(f: &mut Frame<'_>, area: Rect, label: &str, selected: usize, input: &str) {
    let popup_width = 64.min(area.width.saturating_sub(4));
    let popup_height = 12;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let highlight = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let normal = Style::default();

    let options = ["1) Keep (default)", "2) Revert", "3) Other (type message)"];

    let mut lines = Vec::new();
    lines.push(Line::from(""));
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
        format!("Message: {}", input)
    } else {
        "Message: (select Other to type)".to_string()
    };
    lines.push(Line::from(Span::styled(
        message_line,
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Up/Down", Style::default().add_modifier(Modifier::BOLD)),
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

fn draw_config_editor(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    selected: usize,
    editing_value: Option<&str>,
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
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
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
                    value = format!("{}_", editing);
                }
            }
            let label = format!("{:label_width$}", entry.label);
            let line_text = format!("{} {}", label, value);
            let display = truncate_chars(&line_text, list_area.width as usize);

            let mut style = Style::default();
            if entry.value == "(global default)" {
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

fn draw_task_editor(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    selected: usize,
    editing_value: Option<&str>,
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
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
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
                            value = format!("{}_", editing);
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

fn help_footer_spans(app: &App) -> Vec<Span<'static>> {
    let mut help_text = match &app.mode {
        AppMode::Normal => vec![
            Span::styled("?/h", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":help "),
            Span::styled(":", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cmd "),
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
            Span::styled("/", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":search "),
            Span::styled("t", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":tags "),
            Span::styled("f", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":filter "),
            Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":clear "),
            Span::styled("l", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":loop "),
            Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":archive "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":new "),
            Span::styled("g", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":scan "),
            Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":config "),
            Span::styled("s", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cycle "),
            Span::styled("p", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":priority "),
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":refresh"),
        ],
        AppMode::EditingTask { editing_value, .. } => {
            if editing_value.is_some() {
                vec![
                    Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":save "),
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":cancel"),
                ]
            } else {
                vec![
                    Span::styled("Enter/Space", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":edit "),
                    Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":nav "),
                    Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":clear "),
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":close"),
                ]
            }
        }
        AppMode::CreatingTask(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":create "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::Searching(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":search "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::FilteringTags(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":apply "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::FilteringScopes(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":apply "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::EditingConfig { .. } => vec![
            Span::styled("Enter/Space", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":edit "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":nav "),
            Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":clear "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":close"),
        ],
        AppMode::Help => vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":close "),
            Span::styled("?/h", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":close"),
        ],
        AppMode::CommandPalette { .. } => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":run "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":select "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::Scanning(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":scan "),
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
        AppMode::ConfirmArchive => vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":yes "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":no "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::ConfirmQuit => vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":quit "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":stay "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::ConfirmRevert { .. } => vec![
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":select "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":confirm "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":keep"),
        ],
        AppMode::Executing { .. } => vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":return "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":scroll "),
            Span::styled("PgUp/PgDn", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":page "),
            Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":autoscroll "),
            Span::styled("l", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":stop loop"),
        ],
    };

    if app.save_error.is_some() {
        help_text.push(Span::raw(" "));
        help_text.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
        help_text.push(Span::raw(" "));
        help_text.push(Span::styled(
            "SAVE ERROR",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(msg) = app.status_message.as_deref() {
        help_text.push(Span::raw(" "));
        help_text.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
        help_text.push(Span::raw(" "));
        help_text.push(Span::styled(
            msg.to_string(),
            Style::default().fg(Color::Yellow),
        ));
    }

    help_text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::QueueFile;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

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

    #[test]
    fn help_footer_includes_save_error_indicator() {
        let mut app = App::new(QueueFile::default());
        app.save_error = Some("failed to save".to_string());

        let help_text = help_footer_spans(&app);
        let rendered = format!("{:?}", help_text);

        assert!(rendered.contains("SAVE ERROR"));
    }

    #[test]
    fn help_footer_includes_config_hint() {
        let app = App::new(QueueFile::default());
        let help_text = help_footer_spans(&app);
        let rendered = format!("{:?}", help_text);

        assert!(rendered.contains(":config"));
    }

    #[test]
    fn help_footer_includes_scan_hint() {
        let app = App::new(QueueFile::default());
        let help_text = help_footer_spans(&app);
        let rendered = format!("{:?}", help_text);

        assert!(rendered.contains(":scan"));
    }

    #[test]
    fn executing_view_updates_visible_lines_cache() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut app = App::new(QueueFile::default());
        app.mode = AppMode::Executing {
            task_id: "RQ-0001".to_string(),
        };
        app.running_task_id = Some("RQ-0001".to_string());
        app.log_visible_lines = 20;

        terminal.draw(|f| draw_ui(f, &mut app)).expect("draw ui");

        let expected = 10usize.saturating_sub(2).saturating_sub(1).max(1);
        assert_eq!(app.log_visible_lines, expected);
    }

    fn buffer_as_string(buffer: &Buffer) -> String {
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                output.push_str(cell.symbol());
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn empty_queue_renders_action_prompts() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut app = App::new(QueueFile::default());

        terminal.draw(|f| draw_ui(f, &mut app)).expect("draw ui");

        let buffer = terminal.backend().buffer();
        let rendered = buffer_as_string(buffer);

        assert!(rendered.contains("No tasks in queue."));
        assert!(rendered.contains("Press n to create one in the TUI."));
        assert!(rendered.contains("Press : to open the command palette."));
    }
}
