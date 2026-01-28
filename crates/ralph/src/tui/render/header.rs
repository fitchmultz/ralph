//! Header/status bar rendering for the TUI.
//!
//! Responsibilities:
//! - Render a persistent status bar at the top of the TUI showing mode, dirty state,
//!   filters, and runner/loop status.
//! - Provide responsive truncation for narrow terminals.
//!
//! Not handled here:
//! - Footer rendering (see `footer.rs`).
//! - Modal overlays or panel content.
//!
//! Invariants/assumptions:
//! - Header is exactly one row high; content must truncate gracefully.
//! - Colors/styles are consistent with the rest of the TUI.

use super::super::{App, AppMode};
use super::utils::{span_width, spans_width, truncate_spans_with_ellipsis};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Draw the header/status bar.
pub(super) fn draw_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let spans = build_header_spans(app, area.width as usize);

    let paragraph = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Left)
        .style(Style::default().bg(Color::Black).fg(Color::White));

    frame.render_widget(paragraph, area);
}

/// Build header spans from app state, respecting max width.
fn build_header_spans(app: &App, max_width: usize) -> Vec<Span<'static>> {
    if max_width == 0 {
        return Vec::new();
    }

    let mut spans = Vec::new();

    // Mode indicator (always shown)
    spans.push(mode_span(&app.mode, app.running_task_id.as_ref()));

    // Dirty indicators
    let dirty = dirty_spans(app.dirty, app.dirty_done, app.dirty_config);
    if !dirty.is_empty() {
        spans.push(Span::raw(" "));
        spans.extend(dirty);
    }

    // Filter summary (if active)
    if app.has_active_filters() {
        if let Some(filter_str) = filter_summary_compact(app) {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("| {}", filter_str),
                Style::default().fg(Color::Magenta),
            ));
        }
    }

    // Runner/loop status
    let status = status_spans(app);
    if !status.is_empty() {
        spans.push(Span::raw(" "));
        spans.extend(status);
    }

    // Task count (right-aligned via truncation logic)
    let count_span = task_count_span(app);

    // Calculate if we need truncation
    let content_width = spans_width(&spans);
    let count_width = span_width(&count_span);

    if content_width + count_width > max_width {
        // Need to truncate content to make room for count
        let content_budget = max_width.saturating_sub(count_width);
        spans = truncate_spans_with_ellipsis(&spans, content_budget);
    }

    // Add count at the end
    spans.push(count_span);

    spans
}

/// Build mode indicator span.
fn mode_span(mode: &AppMode, running_task_id: Option<&String>) -> Span<'static> {
    match mode {
        AppMode::Normal => Span::styled("[Normal]", Style::default().fg(Color::Cyan)),
        AppMode::Executing { .. } => {
            if let Some(id) = running_task_id {
                Span::styled(
                    format!("[Executing: {}]", id),
                    Style::default().fg(Color::Green),
                )
            } else {
                Span::styled("[Executing]", Style::default().fg(Color::Green))
            }
        }
        AppMode::EditingTask { .. } => {
            Span::styled("[Editing Task]", Style::default().fg(Color::Yellow))
        }
        AppMode::CreatingTask(_) => {
            Span::styled("[Creating Task]", Style::default().fg(Color::Yellow))
        }
        AppMode::CreatingTaskDescription(_) => {
            Span::styled("[Task Builder]", Style::default().fg(Color::Yellow))
        }
        AppMode::Searching(_) => Span::styled("[Searching]", Style::default().fg(Color::Blue)),
        AppMode::FilteringTags(_) => {
            Span::styled("[Filter Tags]", Style::default().fg(Color::Blue))
        }
        AppMode::FilteringScopes(_) => {
            Span::styled("[Filter Scopes]", Style::default().fg(Color::Blue))
        }
        AppMode::Scanning(_) => Span::styled("[Scanning]", Style::default().fg(Color::Blue)),
        AppMode::EditingConfig { .. } => {
            Span::styled("[Editing Config]", Style::default().fg(Color::Yellow))
        }
        AppMode::CommandPalette { .. } => {
            Span::styled("[Command Palette]", Style::default().fg(Color::Blue))
        }
        AppMode::BuildingTaskOptions(_) => {
            Span::styled("[Task Options]", Style::default().fg(Color::Yellow))
        }
        AppMode::JumpingToTask(_) => {
            Span::styled("[Jump to Task]", Style::default().fg(Color::Blue))
        }
        AppMode::Help => Span::styled("[Help]", Style::default().fg(Color::Blue)),
        AppMode::ConfirmDelete => Span::styled("[Confirm Delete]", Style::default().fg(Color::Red)),
        AppMode::ConfirmArchive => {
            Span::styled("[Confirm Archive]", Style::default().fg(Color::Yellow))
        }
        AppMode::ConfirmAutoArchive(_) => {
            Span::styled("[Confirm Archive]", Style::default().fg(Color::Yellow))
        }
        AppMode::ConfirmQuit => Span::styled("[Confirm Quit]", Style::default().fg(Color::Yellow)),
        AppMode::ConfirmDiscard { .. } => {
            Span::styled("[Confirm Discard]", Style::default().fg(Color::Red))
        }
        AppMode::ConfirmRevert { .. } => {
            Span::styled("[Confirm Revert]", Style::default().fg(Color::Yellow))
        }
        AppMode::ConfirmRiskyConfig { .. } => {
            Span::styled("[Confirm Config]", Style::default().fg(Color::Red))
        }
    }
}

/// Build dirty state indicators.
fn dirty_spans(dirty: bool, dirty_done: bool, dirty_config: bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    if dirty {
        spans.push(Span::styled("*queue", Style::default().fg(Color::Yellow)));
    }
    if dirty_done {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled("*done", Style::default().fg(Color::Yellow)));
    }
    if dirty_config {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled("*cfg", Style::default().fg(Color::Yellow)));
    }

    spans
}

/// Build runner/loop status spans.
fn status_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    if app.runner_active {
        if let Some(id) = app.running_task_id.as_deref() {
            spans.push(Span::styled(
                format!("▶ {}", id),
                Style::default().fg(Color::Green),
            ));
        } else {
            spans.push(Span::styled("▶", Style::default().fg(Color::Green)));
        }
    }

    if app.loop_active {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        let loop_text = if let Some(max) = app.loop_max_tasks {
            format!("∞ {}/{}", app.loop_ran, max)
        } else {
            format!("∞ {}", app.loop_ran)
        };
        spans.push(Span::styled(loop_text, Style::default().fg(Color::Cyan)));
    }

    spans
}

/// Build compact filter summary string.
fn filter_summary_compact(app: &App) -> Option<String> {
    let mut parts = Vec::new();

    // Status filters
    if !app.filters.statuses.is_empty() {
        let status_str = if app.filters.statuses.len() == 1 {
            app.filters.statuses[0].as_str().to_string()
        } else {
            format!("status={}", app.filters.statuses.len())
        };
        parts.push(status_str);
    }

    // Tag filters
    if !app.filters.tags.is_empty() {
        let tag_str = if app.filters.tags.len() == 1 {
            format!("tag:{}", app.filters.tags[0])
        } else {
            format!("tags={}", app.filters.tags.len())
        };
        parts.push(tag_str);
    }

    // Scope filters
    if !app.filters.search_options.scopes.is_empty() {
        let scope_str = if app.filters.search_options.scopes.len() == 1 {
            format!("scope:{}", app.filters.search_options.scopes[0])
        } else {
            format!("scopes={}", app.filters.search_options.scopes.len())
        };
        parts.push(scope_str);
    }

    // Search query
    let query = app.filters.query.trim();
    if !query.is_empty() {
        parts.push(format!("q:{}", query));
    }

    // Search options
    if app.filters.search_options.use_regex {
        parts.push("regex".to_string());
    }
    if app.filters.search_options.case_sensitive {
        parts.push("case".to_string());
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// Build task count span (right-aligned).
fn task_count_span(app: &App) -> Span<'static> {
    let total = app.queue.tasks.len();
    let visible = app.filtered_len();

    let text = if app.has_active_filters() {
        format!(" {} / {} ", visible, total)
    } else {
        format!(" {} ", total)
    };

    Span::styled(text, Style::default().fg(Color::DarkGray))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
    use std::collections::HashMap;

    fn make_test_app() -> App {
        App::new(QueueFile::default())
    }

    fn make_test_app_with_tasks() -> App {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                Task {
                    id: "RQ-0001".to_string(),
                    title: "Test Task".to_string(),
                    status: TaskStatus::Todo,
                    priority: TaskPriority::Medium,
                    tags: vec![],
                    scope: vec![],
                    evidence: vec![],
                    plan: vec![],
                    notes: vec![],
                    request: None,
                    agent: None,
                    created_at: None,
                    updated_at: None,
                    completed_at: None,
                    depends_on: vec![],
                    custom_fields: HashMap::new(),
                },
                Task {
                    id: "RQ-0002".to_string(),
                    title: "Another Task".to_string(),
                    status: TaskStatus::Doing,
                    priority: TaskPriority::High,
                    tags: vec![],
                    scope: vec![],
                    evidence: vec![],
                    plan: vec![],
                    notes: vec![],
                    request: None,
                    agent: None,
                    created_at: None,
                    updated_at: None,
                    completed_at: None,
                    depends_on: vec![],
                    custom_fields: HashMap::new(),
                },
            ],
        };
        App::new(queue)
    }

    #[test]
    fn test_mode_span_normal() {
        let span = mode_span(&AppMode::Normal, None);
        assert!(span.content.contains("Normal"));
    }

    #[test]
    fn test_mode_span_executing_with_task() {
        let span = mode_span(
            &AppMode::Executing {
                task_id: "RQ-0001".to_string(),
            },
            Some(&"RQ-0001".to_string()),
        );
        assert!(span.content.contains("Executing"));
        assert!(span.content.contains("RQ-0001"));
    }

    #[test]
    fn test_dirty_spans_all_dirty() {
        let spans = dirty_spans(true, true, true);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("queue"));
        assert!(text.contains("done"));
        assert!(text.contains("cfg"));
    }

    #[test]
    fn test_dirty_spans_none_dirty() {
        let spans = dirty_spans(false, false, false);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_status_spans_runner_only() {
        let mut app = make_test_app();
        app.runner_active = true;
        app.running_task_id = Some("RQ-0001".to_string());

        let spans = status_spans(&app);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("RQ-0001"));
    }

    #[test]
    fn test_status_spans_loop_only() {
        let mut app = make_test_app();
        app.loop_active = true;
        app.loop_ran = 5;
        app.loop_max_tasks = Some(10);

        let spans = status_spans(&app);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("5/10"));
    }

    #[test]
    fn test_status_spans_loop_unlimited() {
        let mut app = make_test_app();
        app.loop_active = true;
        app.loop_ran = 3;
        app.loop_max_tasks = None;

        let spans = status_spans(&app);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("3"));
        assert!(!text.contains('/'));
    }

    #[test]
    fn test_task_count_span_no_filters() {
        let app = make_test_app_with_tasks();
        let span = task_count_span(&app);
        assert!(span.content.contains("2"));
    }

    #[test]
    fn test_build_header_spans_includes_mode() {
        let app = make_test_app();
        let spans = build_header_spans(&app, 80);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Normal"));
    }

    #[test]
    fn test_build_header_spans_with_dirty() {
        let mut app = make_test_app();
        app.dirty = true;
        let spans = build_header_spans(&app, 80);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("*queue"));
    }

    #[test]
    fn test_build_header_spans_truncates_narrow() {
        let mut app = make_test_app_with_tasks();
        app.dirty = true;
        app.dirty_done = true;
        app.dirty_config = true;
        app.runner_active = true;
        app.running_task_id = Some("RQ-0001".to_string());
        app.loop_active = true;
        app.loop_ran = 5;

        // Very narrow width - should still render without panic
        let spans = build_header_spans(&app, 20);
        // Should have at least mode and count
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_filter_summary_compact_empty() {
        let app = make_test_app();
        assert!(filter_summary_compact(&app).is_none());
    }

    #[test]
    fn test_filter_summary_compact_with_status() {
        let mut app = make_test_app();
        app.filters.statuses = vec![TaskStatus::Todo, TaskStatus::Doing];
        let summary = filter_summary_compact(&app).unwrap();
        assert!(summary.contains("status=2"));
    }

    #[test]
    fn test_filter_summary_compact_with_single_status() {
        let mut app = make_test_app();
        app.filters.statuses = vec![TaskStatus::Todo];
        let summary = filter_summary_compact(&app).unwrap();
        assert!(summary.contains("todo"));
        assert!(!summary.contains("status="));
    }

    #[test]
    fn test_filter_summary_compact_with_query() {
        let mut app = make_test_app();
        app.filters.query = "test query".to_string();
        let summary = filter_summary_compact(&app).unwrap();
        assert!(summary.contains("q:test query"));
    }

    #[test]
    fn test_filter_summary_compact_with_tags() {
        let mut app = make_test_app();
        app.filters.tags = vec!["urgent".to_string(), "bug".to_string()];
        let summary = filter_summary_compact(&app).unwrap();
        assert!(summary.contains("tags=2"));
    }

    #[test]
    fn test_filter_summary_compact_with_options() {
        let mut app = make_test_app();
        app.filters.search_options.use_regex = true;
        app.filters.search_options.case_sensitive = true;
        let summary = filter_summary_compact(&app).unwrap();
        assert!(summary.contains("regex"));
        assert!(summary.contains("case"));
    }
}
