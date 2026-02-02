//! Task list panel rendering.
//!
//! Responsibilities:
//! - Render the task list with status, priority, and scheduled indicators.
//! - Display filter summary and runner/loop status in the panel title.
//!
//! Not handled here:
//! - Task details rendering (see `details` module).
//! - Filter logic or task selection state changes.
//!
//! Invariants/assumptions:
//! - Caller provides a valid layout area including borders.
//! - The `App::filtered_indices` is up to date before rendering.

use super::super::App;
use super::{filter_summary_for_width, format_duration_compact, task_list_suffix_spans};
use crate::tui::app_multi_select::MultiSelectOperations;
use crate::tui::app_panel::PanelOperations;
use crate::tui::render::utils::status_color;
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState},
};

/// Draw the task list panel.
pub fn draw_task_list(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let total_count = app.queue.tasks.len();
    let visible_count = app.filtered_len();
    let count_label = if app.has_active_filters() {
        format!("{}/{}", visible_count, total_count)
    } else {
        format!("{}", total_count)
    };
    let mut title_spans = vec![
        Span::styled("Tasks", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" ("),
        Span::styled(count_label, Style::default().fg(Color::DarkGray)),
        Span::raw(")"),
    ];

    let suffix_spans = task_list_suffix_spans(app);
    let title_width = area.width.saturating_sub(2) as usize;
    let base_width = spans_width(&title_spans);
    let suffix_width = spans_width(&suffix_spans);
    let available = title_width.saturating_sub(base_width.saturating_add(suffix_width));

    if available > 1
        && let Some(summary) = filter_summary_for_width(app, available.saturating_sub(1))
    {
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled(summary, Style::default().fg(Color::DarkGray)));
    }

    title_spans.extend(suffix_spans);

    let title = Line::from(title_spans);

    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let list_height = inner.height as usize;
    app.list_height = list_height;
    if inner.width == 0 || inner.height == 0 {
        app.clear_list_area();
    } else {
        app.set_list_area(inner);
    }

    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(list_height)
        .filter_map(|(i, &task_index)| {
            let task = app.queue.tasks.get(task_index)?;
            let is_cursor = i == app.selected;
            let is_multi_selected = app.multi_select_mode && app.is_selected(i);
            let status_style = Style::default().fg(status_color(task.status));

            // Check if task is scheduled for future and build clock indicator
            let scheduled_indicator = task.scheduled_start.as_ref().and_then(|scheduled| {
                crate::timeutil::parse_rfc3339(scheduled)
                    .ok()
                    .and_then(|scheduled_dt| {
                        crate::timeutil::now_utc_rfc3339()
                            .ok()
                            .and_then(|now| crate::timeutil::parse_rfc3339(&now).ok())
                            .map(|now_dt| {
                                if scheduled_dt > now_dt {
                                    let duration = scheduled_dt - now_dt;
                                    format!("⏰ {} ", format_duration_compact(duration))
                                } else {
                                    String::new()
                                }
                            })
                    })
            });

            // Build the line with optional multi-select checkbox
            let mut spans = Vec::new();

            // Add checkbox indicator when in multi-select mode
            if app.multi_select_mode {
                let checkbox = if is_multi_selected { "[x] " } else { "[ ] " };
                let checkbox_style = if is_multi_selected {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                spans.push(Span::styled(checkbox, checkbox_style));
            }

            if is_cursor {
                spans.push(Span::styled(
                    &task.id,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    task.status.as_str(),
                    status_style.add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    task.priority.as_str(),
                    Style::default().fg(Color::DarkGray),
                ));
                spans.push(Span::raw(" "));
                // Add clock indicator if scheduled
                if let Some(ref indicator) = scheduled_indicator
                    && !indicator.is_empty()
                {
                    spans.push(Span::styled(
                        indicator.clone(),
                        Style::default().fg(Color::Yellow),
                    ));
                }
                spans.push(Span::styled(
                    &task.title,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(&task.id, Style::default().fg(Color::DarkGray)));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(task.status.as_str(), status_style));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    task.priority.as_str(),
                    Style::default().fg(Color::DarkGray),
                ));
                spans.push(Span::raw(" "));
                // Add clock indicator if scheduled
                if let Some(ref indicator) = scheduled_indicator
                    && !indicator.is_empty()
                {
                    spans.push(Span::styled(
                        indicator.clone(),
                        Style::default().fg(Color::Yellow),
                    ));
                }
                spans.push(Span::styled(&task.title, Style::default()));
            }

            let line = Line::from(spans);
            Some(ListItem::new(line))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().bg(Color::Blue))
        .highlight_symbol("» ")
        .highlight_spacing(HighlightSpacing::Always);

    let visible_count = list_height.min(app.filtered_len());
    let selected_in_view = if visible_count == 0 {
        None
    } else if app.selected >= app.scroll && app.selected < app.scroll + visible_count {
        Some(app.selected.saturating_sub(app.scroll))
    } else {
        None
    };
    let mut state = ListState::default();
    state.select(selected_in_view);

    f.render_stateful_widget(list, area, &mut state);
}

/// Calculate the total width of a slice of spans.
fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.content.len()).sum()
}
