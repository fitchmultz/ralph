//! Shared rendering helpers for the TUI.
//!
//! Responsibilities:
//! - Provide text wrapping and color helpers for panel rendering.
//! - Provide small formatting helpers used by multiple renderers.
//!
//! Not handled here:
//! - Layout logic or widget composition.
//! - Event handling or state mutation.
//!
//! Invariants/assumptions:
//! - Callers clamp input widths before rendering to avoid zero-width layouts.

use crate::contracts::{TaskPriority, TaskStatus};
use crate::output::theme::tui as theme_colors;
use crate::outpututil::truncate_chars;
use ratatui::{style::Color, text::Span};

/// Wrap text to fit within a given width.
pub(super) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    // `textwrap` requires a non-zero width. Extremely small layouts can yield 0
    // (e.g., panel width smaller than padding). Clamp to keep rendering resilient.
    let width = width.max(1);

    textwrap::wrap(text, width)
        .into_iter()
        .map(|s| s.into_owned())
        .collect()
}

/// Get the color for a task status.
pub(super) fn status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Draft => Color::DarkGray,
        TaskStatus::Todo => Color::Blue,
        TaskStatus::Doing => Color::Yellow,
        TaskStatus::Done => Color::Green,
        TaskStatus::Rejected => Color::Red,
    }
}

/// Get the color for a task priority.
pub(super) fn priority_color(priority: TaskPriority) -> Color {
    match priority {
        TaskPriority::Critical => Color::Red,
        TaskPriority::High => Color::Yellow,
        TaskPriority::Medium => Color::Blue,
        TaskPriority::Low => Color::DarkGray,
    }
}

/// Format a scroll indicator string when content exceeds the viewport.
pub(super) fn scroll_indicator(
    scroll: usize,
    visible_lines: usize,
    total_lines: usize,
) -> Option<String> {
    if total_lines <= visible_lines {
        return None;
    }

    let start = scroll.saturating_add(1);
    let end = (scroll + visible_lines).min(total_lines);
    let percent = if total_lines == 0 {
        0
    } else {
        (end.saturating_mul(100)) / total_lines
    };
    Some(format!("({start}-{end}/{total_lines}, {percent}%)"))
}

pub(super) fn truncate_spans_with_ellipsis(
    spans: &[Span<'static>],
    max_width: usize,
) -> Vec<Span<'static>> {
    if max_width == 0 {
        return Vec::new();
    }

    if spans_width(spans) <= max_width {
        return spans.to_vec();
    }

    let ellipsis = "...";
    let ellipsis_width = ellipsis.len();
    if max_width <= ellipsis_width {
        return vec![Span::raw(truncate_chars(ellipsis, max_width))];
    }

    let target_width = max_width.saturating_sub(ellipsis_width);
    let mut out = Vec::new();
    let mut used = 0usize;

    for span in spans {
        let width = span_width(span);
        if used + width <= target_width {
            out.push(span.clone());
            used += width;
            continue;
        }

        let remaining = target_width.saturating_sub(used);
        if remaining > 0 {
            out.push(truncate_span(span, remaining));
        }
        break;
    }

    out.push(Span::raw(ellipsis));
    out
}

pub(super) fn spans_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(span_width).sum()
}

pub(super) fn span_width(span: &Span<'static>) -> usize {
    span.content.chars().count()
}

fn truncate_span(span: &Span<'static>, max_width: usize) -> Span<'static> {
    let truncated = truncate_chars(span.content.as_ref(), max_width);
    Span::styled(truncated, span.style)
}

// Runner output colors for TUI
// These are available for future TUI enhancements to colorize runner output

/// Get the color for agent reasoning/thinking blocks
#[allow(dead_code)]
pub(super) fn reasoning_color() -> Color {
    theme_colors::reasoning()
}

/// Get the color for tool calls
#[allow(dead_code)]
pub(super) fn tool_call_color() -> Color {
    theme_colors::tool_call()
}

/// Get the color for successful tool results
#[allow(dead_code)]
pub(super) fn tool_result_success_color() -> Color {
    theme_colors::tool_result_success()
}

/// Get the color for failed tool results
#[allow(dead_code)]
pub(super) fn tool_result_error_color() -> Color {
    theme_colors::tool_result_error()
}

/// Get the color for command execution
#[allow(dead_code)]
pub(super) fn command_color() -> Color {
    theme_colors::command()
}

/// Get the color for supervisor/system messages
#[allow(dead_code)]
pub(super) fn supervisor_color() -> Color {
    theme_colors::supervisor()
}
