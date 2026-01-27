//! Footer rendering for the TUI.
//!
//! Responsibilities:
//! - Render the bottom help/status line for each TUI mode.
//! - Surface save errors and status messages alongside key hints.
//!
//! Not handled here:
//! - Main panel rendering or modal overlays.
//! - Input handling logic.
//!
//! Invariants/assumptions:
//! - Footer text is short enough to truncate gracefully on narrow terminals.
//! - Caller supplies the footer area row.

use super::super::events::types::ConfirmDiscardAction;
use super::super::{keymap, App, AppMode};
use crate::outpututil::truncate_chars;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Draw the footer area.
pub(super) fn draw_footer(f: &mut Frame<'_>, app: &App, area: Rect) {
    let help_text = help_footer_spans(app, area.width as usize);

    let help_paragraph = Paragraph::new(Line::from(help_text))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    f.render_widget(help_paragraph, area);
}

pub(super) fn help_footer_spans(app: &App, max_width: usize) -> Vec<Span<'static>> {
    let mut help_text = match &app.mode {
        AppMode::Normal | AppMode::Executing { .. } | AppMode::Help => {
            footer_spans_from_hints(&keymap::footer_hints_for_mode(&app.mode))
        }
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
        AppMode::CreatingTaskDescription(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":build "),
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
        AppMode::ConfirmDiscard { action } => {
            let yes_label = match action {
                ConfirmDiscardAction::ReloadQueue => "reload",
                ConfirmDiscardAction::Quit => "quit",
            };
            vec![
                Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(":{} ", yes_label)),
                Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":cancel "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":cancel"),
            ]
        }
        AppMode::ConfirmRevert { .. } => vec![
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":select "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":confirm "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":keep"),
        ],
    };

    let tail_spans = build_tail_spans(app, max_width, !help_text.is_empty());
    let tail_width = spans_width(&tail_spans);
    let hint_budget = max_width.saturating_sub(tail_width);
    help_text = truncate_spans_with_ellipsis(&help_text, hint_budget);

    help_text.extend(tail_spans);
    help_text
}

fn footer_spans_from_hints(hints: &[keymap::FooterHint]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for hint in hints {
        spans.push(Span::styled(
            hint.keys,
            Style::default().add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(":{} ", hint.label)));
    }
    trim_trailing_space(&mut spans);
    spans
}

fn trim_trailing_space(spans: &mut Vec<Span<'static>>) {
    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end_matches(' ').to_string();
        if trimmed.len() != last.content.len() {
            *last = Span::styled(trimmed, last.style);
        }
    }
}

fn build_tail_spans(app: &App, max_width: usize, include_separator: bool) -> Vec<Span<'static>> {
    if max_width == 0 {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut remaining = max_width;
    let mut needs_separator = include_separator;

    if app.save_error.is_some() {
        if !push_separator(&mut spans, &mut remaining, needs_separator) {
            return spans;
        }
        let label = truncate_chars("SAVE ERROR", remaining);
        remaining = remaining.saturating_sub(label.chars().count());
        spans.push(Span::styled(
            label,
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        needs_separator = true;
    }

    if let Some(msg) = app.status_message.as_deref() {
        if !push_separator(&mut spans, &mut remaining, needs_separator) {
            return spans;
        }
        let label = truncate_chars(msg, remaining);
        spans.push(Span::styled(label, Style::default().fg(Color::Yellow)));
    }

    spans
}

fn push_separator(
    spans: &mut Vec<Span<'static>>,
    remaining: &mut usize,
    include_separator: bool,
) -> bool {
    if !include_separator {
        return true;
    }
    if *remaining < 3 {
        return false;
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
    spans.push(Span::raw(" "));
    *remaining = remaining.saturating_sub(3);
    true
}

fn truncate_spans_with_ellipsis(spans: &[Span<'static>], max_width: usize) -> Vec<Span<'static>> {
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

fn truncate_span(span: &Span<'static>, max_width: usize) -> Span<'static> {
    let truncated = truncate_chars(span.content.as_ref(), max_width);
    Span::styled(truncated, span.style)
}

fn spans_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(span_width).sum()
}

fn span_width(span: &Span<'static>) -> usize {
    span.content.chars().count()
}
