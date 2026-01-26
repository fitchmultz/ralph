use super::super::{App, AppMode};
use super::utils::{priority_color, status_color, wrap_text};
use crate::contracts::{TaskPriority, TaskStatus};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Draw the execution view (full-screen output during task execution).
pub(super) fn draw_execution_view(f: &mut Frame<'_>, app: &mut App, area: Rect) {
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

    // Render logs (avoid an intermediate Vec<&String> allocation).
    let log_text = Text::from(
        app.logs
            .iter()
            .skip(start_idx)
            .take(visible_height)
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

/// Draw the task list panel.
pub(super) fn draw_task_list(f: &mut Frame<'_>, app: &mut App, area: Rect) {
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
            title_spans.push(Span::styled(id, Style::default().fg(Color::Cyan)));
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
pub(super) fn draw_task_details(f: &mut Frame<'_>, app: &mut App, area: Rect) {
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
        AppMode::CreatingTaskDescription(description) => Line::from(vec![
            Span::styled(
                "Task Builder: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                description,
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

    if let AppMode::CreatingTaskDescription(current) = &app.mode {
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "Task Description",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(":", Style::default()),
            ]),
            Line::from(""),
        ];
        let display = if current.is_empty() {
            "(describe task you want to create, agent will add structure)"
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
            "The task builder agent will create a structured task with proper scoping, evidence, and planning.",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Enter to build task or Esc to cancel.",
            Style::default().fg(Color::DarkGray),
        )));
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
                Span::styled(": ", Style::default()),
            ]));
            // Note: We inline custom fields sorting/formatting here (rather than using
            // format_custom_fields from outpututil) because we need per-line wrapping
            // for TUI display. The format_custom_fields helper returns a single
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
                "Press n to create one in TUI.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
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
        Line::from(""),
        Line::from(Span::styled(
            "Press / to search or t to filter tags.",
            Style::default().fg(Color::DarkGray),
        )),
    ]);
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}
