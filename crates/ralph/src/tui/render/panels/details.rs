//! Task details panel rendering.
//!
//! Responsibilities:
//! - Render task details for the selected task.
//! - Handle multiple AppMode variants (CreatingTask, EditingTask, Searching, etc.).
//! - Display empty queue and filtered empty states.
//!
//! Not handled here:
//! - Task list rendering (see `list` module).
//! - Scroll view management (handled by `render_details_panel` in mod.rs).
//!
//! Invariants/assumptions:
//! - Caller provides a valid layout area including borders.
//! - `app.detail_width` is updated before text wrapping operations.

use super::super::{App, AppMode};
use super::{filter_summary_for_width, render_details_panel, DetailsPanelContent};
use crate::contracts::{TaskPriority, TaskStatus};
use crate::tui::render::utils::{priority_color, status_color, wrap_text};
use crate::tui::DetailsContextMode;
use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};

/// Draw the task details panel.
pub fn draw_task_details(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    app.detail_width = area.width.saturating_sub(4); // Account for borders

    let title_spans = match &app.mode {
        AppMode::EditingTask { .. } => vec![Span::styled(
            "Task Editor",
            Style::default().add_modifier(Modifier::BOLD),
        )],
        AppMode::CreatingTask(title) => vec![
            Span::styled("New Task: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                title.with_cursor_marker('_'),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        AppMode::CreatingTaskDescription(description) => vec![
            Span::styled(
                "Task Builder: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                description.with_cursor_marker('_'),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        AppMode::Searching(query) => vec![
            Span::styled("Search: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                query.with_cursor_marker('_'),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        AppMode::FilteringTags(tags) => vec![
            Span::styled(
                "Filter Tags: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                tags.with_cursor_marker('_'),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        AppMode::FilteringScopes(scopes) => vec![
            Span::styled(
                "Filter Scopes: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                scopes.with_cursor_marker('_'),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        AppMode::Scanning(focus) => vec![
            Span::styled(
                "Scan Focus: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                focus.with_cursor_marker('_'),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        AppMode::CommandPalette { .. } => vec![Span::styled(
            "Task Details",
            Style::default().add_modifier(Modifier::BOLD),
        )],
        _ => vec![Span::styled(
            "Task Details",
            Style::default().add_modifier(Modifier::BOLD),
        )],
    };

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

        let title_text = if current.value().is_empty() {
            "(enter a title)".to_string()
        } else {
            current.value().to_string()
        };
        for line in wrap_text(&title_text, app.detail_width as usize) {
            let style = if current.value().is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }
        render_details_panel(
            f,
            app,
            area,
            inner,
            DetailsPanelContent {
                title_spans: title_spans.clone(),
                lines,
                context_mode: DetailsContextMode::CreatingTask,
                selected_id: None,
            },
        );
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
        let display = if current.value().is_empty() {
            "(describe task you want to create, agent will add structure)".to_string()
        } else {
            current.value().to_string()
        };
        for line in wrap_text(&display, app.detail_width as usize) {
            let style = if current.value().is_empty() {
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
        render_details_panel(
            f,
            app,
            area,
            inner,
            DetailsPanelContent {
                title_spans: title_spans.clone(),
                lines,
                context_mode: DetailsContextMode::CreatingTaskDescription,
                selected_id: None,
            },
        );
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
        let display = if current.value().is_empty() {
            "(type to search across title, tags, scope, plan, evidence, notes)".to_string()
        } else {
            current.value().to_string()
        };
        for line in wrap_text(&display, app.detail_width as usize) {
            let style = if current.value().is_empty() {
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
        render_details_panel(
            f,
            app,
            area,
            inner,
            DetailsPanelContent {
                title_spans: title_spans.clone(),
                lines,
                context_mode: DetailsContextMode::Searching,
                selected_id: None,
            },
        );
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
        let display = if current.value().is_empty() {
            "(e.g., tui, ux, docs)".to_string()
        } else {
            current.value().to_string()
        };
        for line in wrap_text(&display, app.detail_width as usize) {
            let style = if current.value().is_empty() {
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
        render_details_panel(
            f,
            app,
            area,
            inner,
            DetailsPanelContent {
                title_spans: title_spans.clone(),
                lines,
                context_mode: DetailsContextMode::FilteringTags,
                selected_id: None,
            },
        );
        return;
    }

    if let AppMode::FilteringScopes(current) = &app.mode {
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "Scopes",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(" (comma-separated):", Style::default()),
            ]),
            Line::from(""),
        ];
        let display = if current.value().is_empty() {
            "(e.g., src/, docs/)".to_string()
        } else {
            current.value().to_string()
        };
        for line in wrap_text(&display, app.detail_width as usize) {
            let style = if current.value().is_empty() {
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
        render_details_panel(
            f,
            app,
            area,
            inner,
            DetailsPanelContent {
                title_spans: title_spans.clone(),
                lines,
                context_mode: DetailsContextMode::FilteringTags,
                selected_id: None,
            },
        );
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
        let display = if current.value().is_empty() {
            "(optional: describe what to scan for)".to_string()
        } else {
            current.value().to_string()
        };
        for line in wrap_text(&display, app.detail_width as usize) {
            let style = if current.value().is_empty() {
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
        render_details_panel(
            f,
            app,
            area,
            inner,
            DetailsPanelContent {
                title_spans: title_spans.clone(),
                lines,
                context_mode: DetailsContextMode::Scanning,
                selected_id: None,
            },
        );
        return;
    }

    if let Some(task) = app.selected_task() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    task.id.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
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
                task.created_at.clone().unwrap_or_else(|| "N/A".to_string()),
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
                task.updated_at.clone().unwrap_or_else(|| "N/A".to_string()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        let selected_id = task.id.clone();
        render_details_panel(
            f,
            app,
            area,
            inner,
            DetailsPanelContent {
                title_spans: title_spans.clone(),
                lines,
                context_mode: DetailsContextMode::TaskDetails,
                selected_id: Some(selected_id),
            },
        );
        return;
    }

    if app.queue.tasks.is_empty() {
        let lines = vec![
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
        ];
        render_details_panel(
            f,
            app,
            area,
            inner,
            DetailsPanelContent {
                title_spans: title_spans.clone(),
                lines,
                context_mode: DetailsContextMode::EmptyQueue,
                selected_id: None,
            },
        );
        return;
    }

    let filter_hint = filter_summary_for_width(app, inner.width as usize)
        .unwrap_or_else(|| "filters active".to_string());
    let filter_summary = filter_hint.clone();
    let lines = vec![
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
    ];
    render_details_panel(
        f,
        app,
        area,
        inner,
        DetailsPanelContent {
            title_spans,
            lines,
            context_mode: DetailsContextMode::FilteredEmpty {
                summary: filter_summary,
            },
            selected_id: None,
        },
    );
}
