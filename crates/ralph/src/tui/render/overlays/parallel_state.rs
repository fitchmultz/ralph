//! Parallel state overlay rendering.
//!
//! Responsibilities:
//! - Render a read-only overlay for `.ralph/cache/parallel/state.json`.
//! - Display concise tables for: in-flight tasks, PR records (incl merge blockers), and finished-without-PR.
//! - Display missing/invalid state guidance without crashing.
//!
//! Not handled here:
//! - Event handling (see `tui::events::parallel_state`).
//! - Loading state from disk (handled by `App` overlay helpers).
//!
//! Invariants/assumptions:
//! - The view is read-only; rendering must not mutate parallel execution state.
//! - `App` provides a snapshot of loaded state (or missing/invalid) plus overlay UI state.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Row, Table, Tabs},
};

use crate::outpututil::truncate_chars;
use crate::tui::App;
use crate::tui::app_parallel_state::{ParallelStateOverlaySnapshot, ParallelStateTab};

/// Draw the parallel state overlay.
pub fn draw_parallel_state_overlay(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let popup = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    f.render_widget(Clear, popup);

    let title = Line::from(vec![
        Span::styled(
            "Parallel Run State",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            "(read-only)",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ]);

    let block = Block::default().title(title).borders(Borders::ALL);
    f.render_widget(block.clone(), popup);
    let inner = block.inner(popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(2), // metadata line
                Constraint::Length(1), // tabs
                Constraint::Min(1),    // body
                Constraint::Length(1), // footer hint
            ]
            .as_ref(),
        )
        .split(inner);

    draw_metadata(f, app, layout[0]);
    draw_tabs(f, app, layout[1]);
    draw_body(f, app, layout[2]);
    draw_footer(f, app, layout[3]);
}

fn draw_metadata(f: &mut Frame<'_>, app: &App, area: Rect) {
    let meta = app.parallel_state_overlay_metadata_line(area.width as usize);
    f.render_widget(Paragraph::new(meta), area);
}

fn draw_tabs(f: &mut Frame<'_>, app: &App, area: Rect) {
    let (counts, active) = app.parallel_state_overlay_tab_counts_and_active();

    let titles: Vec<Line<'static>> = vec![
        Line::from(format!("In-Flight ({})", counts.in_flight)),
        Line::from(format!("PRs ({})", counts.prs)),
        Line::from(format!("No PR ({})", counts.finished_without_pr)),
    ];

    let tabs = Tabs::new(titles)
        .select(active.idx())
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(Color::Gray));
    f.render_widget(tabs, area);
}

fn draw_body(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let snapshot = app.parallel_state_overlay_snapshot();

    if area.width == 0 || area.height == 0 {
        return;
    }

    match snapshot {
        ParallelStateOverlaySnapshot::Missing { path } => {
            let text = Text::from(vec![
                Line::from(vec![Span::styled(
                    "No parallel state file found.",
                    Style::default().fg(Color::Yellow),
                )]),
                Line::from(""),
                Line::from(format!("Expected: {}", path)),
                Line::from(""),
                Line::from("Start a parallel run with:"),
                Line::from(vec![Span::styled(
                    "  ralph run loop --parallel",
                    Style::default().add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from("Press `r` to retry loading."),
            ]);
            f.render_widget(Paragraph::new(text), area);
        }
        ParallelStateOverlaySnapshot::Invalid { path, error } => {
            let text = Text::from(vec![
                Line::from(vec![Span::styled(
                    "Failed to parse parallel state file.",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from(format!("File: {}", path)),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Error: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(truncate_chars(&error, area.width as usize)),
                ]),
                Line::from(""),
                Line::from("Press `r` to reload after fixing the file."),
            ]);
            f.render_widget(Paragraph::new(text), area);
        }
        ParallelStateOverlaySnapshot::Loaded { state } => {
            match app.parallel_state_overlay_active_tab() {
                ParallelStateTab::InFlight => draw_in_flight_table(f, app, area, &state),
                ParallelStateTab::Prs => draw_prs_table(f, app, area, &state),
                ParallelStateTab::FinishedWithoutPr => draw_finished_table(f, app, area, &state),
            }
        }
    }
}

fn draw_in_flight_table(
    f: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    state: &crate::commands::run::ParallelStateFile,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("In-Flight Tasks");
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let rows: Vec<Row> = state
        .tasks_in_flight
        .iter()
        .map(|t| {
            Row::new(vec![
                cell(&t.task_id),
                cell(truncate_chars(&t.workspace_path, inner.width as usize)),
                cell(&t.branch),
                cell(
                    t.pid
                        .map(|p: u32| p.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ])
        })
        .collect();

    app.parallel_state_overlay_set_visible_rows(inner.height as usize);

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(22),
            Constraint::Length(6),
        ],
    )
    .header(
        Row::new(vec!["Task", "Workspace", "Branch", "PID"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .column_spacing(1);

    f.render_widget(table, inner);
}

fn draw_prs_table(
    f: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    state: &crate::commands::run::ParallelStateFile,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Pull Requests");
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let selected = app.parallel_state_overlay_selected_pr_index();
    let total = state.prs.len();

    // Derive viewport height for rows (minus header).
    let header_h = 1usize;
    let viewport = (inner.height as usize).saturating_sub(header_h).max(1);
    app.parallel_state_overlay_set_visible_rows(viewport);

    let scroll = app.parallel_state_overlay_pr_scroll();
    let scroll = scroll.min(total);
    let end = (scroll + viewport).min(total);

    let rows: Vec<Row> = state.prs[scroll..end]
        .iter()
        .enumerate()
        .map(|(idx, pr)| {
            let abs_idx = scroll + idx;
            let is_selected = abs_idx == selected;

            let lifecycle = format_pr_lifecycle(&pr.lifecycle, pr.merged);
            let blocker = pr.merge_blocker.as_deref().unwrap_or("-");
            let url = truncate_chars(&pr.pr_url, inner.width as usize);

            let mut row = Row::new(vec![
                cell(&pr.task_id),
                cell(format!("#{}", pr.pr_number)),
                cell(&lifecycle),
                cell(blocker),
                cell(url),
            ]);

            if is_selected {
                row = row.style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            }

            row
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Min(14),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(vec!["Task", "PR", "State", "Merge Blocker", "URL"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .column_spacing(1);

    f.render_widget(table, inner);
}

fn draw_finished_table(
    f: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    state: &crate::commands::run::ParallelStateFile,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Finished Without PR");
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let rows: Vec<Row> = state
        .finished_without_pr
        .iter()
        .map(|r| {
            let result = if r.success { "ok" } else { "fail" };
            let reason = r.reason.as_str();
            let msg = r.message.as_deref().unwrap_or("-");
            Row::new(vec![
                cell(&r.task_id),
                cell(result),
                cell(reason),
                cell(truncate_chars(msg, inner.width as usize)),
            ])
            .style(if r.success {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            })
        })
        .collect();

    app.parallel_state_overlay_set_visible_rows(inner.height as usize);

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(26),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(vec!["Task", "Result", "Reason", "Message"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .column_spacing(1);

    f.render_widget(table, inner);
}

fn draw_footer(f: &mut Frame<'_>, app: &App, area: Rect) {
    let hint = app.parallel_state_overlay_footer_hint();
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn cell(s: impl AsRef<str>) -> ratatui::widgets::Cell<'static> {
    ratatui::widgets::Cell::from(s.as_ref().to_string())
}

fn format_pr_lifecycle(
    lifecycle: &crate::commands::run::ParallelPrLifecycle,
    merged_flag: bool,
) -> String {
    if merged_flag {
        return "merged".to_string();
    }
    match lifecycle {
        crate::commands::run::ParallelPrLifecycle::Open => "open".to_string(),
        crate::commands::run::ParallelPrLifecycle::Closed => "closed".to_string(),
        crate::commands::run::ParallelPrLifecycle::Merged => "merged".to_string(),
    }
}
