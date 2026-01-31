//! TUI panel rendering for task list, task details, and execution view.
//!
//! Responsibilities:
//! - Render the main task list and task details panels.
//! - Render the execution log view during task runs using tui-term for ANSI-aware display.
//!
//! Not handled here:
//! - Event handling or state mutation beyond layout caches.
//! - Modal overlays (see `overlays`).
//!
//! Invariants/assumptions:
//! - Caller provides layout areas that include borders.
//! - ANSI buffer in App contains raw terminal output for tui-term rendering.

use super::super::{App, DetailsContext, DetailsContextMode};
use super::utils::{scroll_indicator, wrap_text};
use crate::outpututil::truncate_chars;
use ratatui::{
    layout::{Rect, Size},
    prelude::StatefulWidget,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use tui_scrollview::{ScrollView, ScrollbarVisibility};

mod details;
mod exec;
mod list;
mod progress;

pub use details::draw_task_details;
pub use exec::draw_execution_view;
pub use list::draw_task_list;

/// Format a duration as a compact string (e.g., "2h", "30m", "45s").
pub(crate) fn format_duration_compact(duration: time::Duration) -> String {
    let secs = duration.whole_seconds();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// Build suffix spans for the task list title based on runner and loop state.
pub(crate) fn task_list_suffix_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    if app.runner_active {
        push_title_separator(&mut spans);
        spans.push(Span::styled(
            "RUNNING",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
        if let Some(id) = app.running_task_id.as_deref() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                id.to_string(),
                Style::default().fg(Color::Cyan),
            ));
        }
    }

    if app.loop_active {
        push_title_separator(&mut spans);
        spans.push(Span::styled(
            "LOOP",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("ran {}", app.loop_ran),
            Style::default().fg(Color::Yellow),
        ));
        if let Some(max) = app.loop_max_tasks {
            spans.push(Span::raw("/"));
            spans.push(Span::styled(
                format!("{}", max),
                Style::default().fg(Color::Yellow),
            ));
        }
    }

    spans
}

/// Push a separator (" | ") into the spans vector.
pub(crate) fn push_title_separator(spans: &mut Vec<Span<'static>>) {
    spans.push(Span::raw(" "));
    spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
    spans.push(Span::raw(" "));
}

/// Generate a filter summary string that fits within the given width.
pub(crate) fn filter_summary_for_width(app: &App, max_width: usize) -> Option<String> {
    if max_width == 0 || !app.has_active_filters() {
        return None;
    }

    let mut parts = Vec::new();

    match app.filters.statuses.len() {
        0 => {}
        1 => {
            let status = app.filters.statuses[0].as_str();
            parts.push(format!("status={status}"));
        }
        count => parts.push(format!("status={count}")),
    }

    match app.filters.tags.len() {
        0 => {}
        1 => parts.push(format!("tags={}", app.filters.tags[0])),
        count => parts.push(format!("tags={count}")),
    }

    let query = app.filters.query.trim();
    if !query.is_empty() {
        parts.push(format!("query={query}"));
    }

    match app.filters.search_options.scopes.len() {
        0 => {}
        1 => parts.push(format!("scope={}", app.filters.search_options.scopes[0])),
        count => parts.push(format!("scopes={count}")),
    }

    if app.filters.search_options.use_regex {
        parts.push("regex".to_string());
    }
    if app.filters.search_options.case_sensitive {
        parts.push("case-sensitive".to_string());
    }

    if parts.is_empty() {
        return None;
    }

    let summary = format!("filters: {}", parts.join(" "));
    Some(truncate_chars(&summary, max_width))
}

/// Content for the details panel, used by `render_details_panel`.
pub(crate) struct DetailsPanelContent {
    pub title_spans: Vec<Span<'static>>,
    pub lines: Vec<Line<'static>>,
    pub context_mode: DetailsContextMode,
    pub selected_id: Option<String>,
}

/// Count the number of wrapped lines for the given content and width.
pub(crate) fn wrapped_line_count(lines: &[Line<'static>], width: usize) -> usize {
    let width = width.max(1);
    lines
        .iter()
        .map(|line| {
            let raw: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            wrap_text(&raw, width).len().max(1)
        })
        .sum()
}

/// Render the details panel with scrollable content.
pub(crate) fn render_details_panel(
    f: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    inner: Rect,
    content: DetailsPanelContent,
) {
    let total_lines = wrapped_line_count(&content.lines, inner.width as usize);
    let visible_lines = inner.height as usize;
    let context = DetailsContext {
        mode: content.context_mode,
        selected_id: content.selected_id,
        queue_rev: app.queue_rev(),
        detail_width: app.detail_width,
    };
    app.set_details_viewport(visible_lines, total_lines, context);

    // Build scroll indicator from current scroll state
    let scroll_y = app.details_scroll();
    let indicator = scroll_indicator(scroll_y, visible_lines, total_lines);
    let title = details_title(content.title_spans, indicator);

    let block = Block::default().title(title).borders(Borders::ALL);
    f.render_widget(block, area);

    // Calculate content size for ScrollView
    // Width: inner.width, Height: total_lines (as u16, capped)
    let content_height = total_lines.max(visible_lines) as u16;
    let content_size = Size::new(inner.width, content_height);

    // Create ScrollView with content size
    // Disable scrollbars to avoid panics with tiny terminal sizes
    let mut scroll_view = ScrollView::new(content_size)
        .vertical_scrollbar_visibility(ScrollbarVisibility::Never)
        .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

    // Render the content into the ScrollView at position (0, 0) within the scroll view
    // The area should cover the full content height
    let content_area = Rect::new(0, 0, inner.width, content_height);
    let paragraph = Paragraph::new(Text::from(content.lines)).wrap(Wrap { trim: false });
    scroll_view.render_widget(paragraph, content_area);

    // Render the ScrollView with the current scroll state
    // Note: tui-scrollview 0.5 uses render() method, not render_stateful_widget
    scroll_view.render(inner, f.buffer_mut(), app.details_scroll_state());
}

/// Build the title line for the details panel, optionally with a scroll indicator.
pub(crate) fn details_title(
    mut spans: Vec<Span<'static>>,
    indicator: Option<String>,
) -> Line<'static> {
    if let Some(indicator) = indicator {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            indicator,
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}
