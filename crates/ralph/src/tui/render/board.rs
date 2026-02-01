//! Kanban board rendering for the TUI.
//!
//! Responsibilities:
//! - Render the Kanban board view with status columns.
//! - Render task cards with priority-based color coding.
//! - Handle board-specific layout calculations.
//!
//! Not handled here:
//! - List view rendering (see `panels.rs`).
//! - Task details panel (shown alongside board in wide mode).
//! - Event handling or navigation logic.
//!
//! Invariants/assumptions:
//! - Columns are always visible even if empty.
//! - Minimum column width is enforced; board falls back to list view if too narrow.
//! - Task cards show truncated title and ID with priority-colored borders.

use super::super::App;
use super::super::events::types::ViewMode;
use super::utils::{priority_color, status_color};
use crate::constants::ui::{BOARD_MIN_WIDTH, COLUMN_GUTTER};
use crate::contracts::TaskStatus;
use crate::outpututil::truncate_chars;
use crate::tui::app_navigation::BoardNavigationState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Draw the Kanban board view.
///
/// Renders a horizontal layout with columns for each task status.
/// Falls back to list view if the terminal is too narrow.
pub fn draw_kanban_board(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    // Check if we have enough space for the board
    if area.width < BOARD_MIN_WIDTH {
        // Fall back to list view with a message
        draw_board_too_narrow(f, app, area);
        return;
    }

    // Split area into columns
    let column_layout = calculate_column_layout(area);

    // Draw each column
    for (column_idx, column_area) in column_layout.iter().enumerate() {
        if let Some(status) = BoardNavigationState::column_to_status(column_idx) {
            let is_selected =
                app.view_mode == ViewMode::Board && app.board_nav.selected_column == column_idx;
            draw_column(f, app, *column_area, status, column_idx, is_selected);
        }
    }
}

/// Draw a fallback message when the terminal is too narrow for the board.
fn draw_board_too_narrow(f: &mut Frame<'_>, _app: &App, area: Rect) {
    let message = format!(
        "Terminal too narrow for board view ({} cols). \
         Press 'l' to switch to list view or resize terminal.",
        area.width
    );
    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(Color::Yellow))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Board View")
                .title_style(Style::default().add_modifier(Modifier::BOLD)),
        );
    f.render_widget(paragraph, area);
}

/// Calculate the layout for board columns.
///
/// Distributes available width evenly across 5 columns with gutters.
fn calculate_column_layout(area: Rect) -> Vec<Rect> {
    let num_columns = 5u16;
    let total_gutter = COLUMN_GUTTER * (num_columns - 1);
    let usable_width = area.width.saturating_sub(total_gutter);
    let column_width = usable_width / num_columns;

    // Create constraints for each column
    let constraints: Vec<Constraint> = (0..num_columns)
        .map(|i| {
            if i < num_columns - 1 {
                Constraint::Length(column_width)
            } else {
                // Last column takes remaining space
                Constraint::Min(column_width)
            }
        })
        .collect();

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    // Convert to Vec and add gutters between columns by shrinking each column except the last
    let mut result: Vec<Rect> = layout.to_vec();
    for i in 0..result.len().saturating_sub(1) {
        result[i] = Rect {
            width: result[i].width.saturating_sub(COLUMN_GUTTER),
            ..result[i]
        };
    }

    result
}

/// Draw a single column in the board.
fn draw_column(
    f: &mut Frame<'_>,
    app: &App,
    area: Rect,
    status: TaskStatus,
    column_idx: usize,
    is_selected: bool,
) {
    let column_count = app.board_nav.column_count(column_idx);

    // Build column title with count badge
    let title = format!("{} ({})", status.as_str(), column_count);
    let status_style = Style::default()
        .fg(status_color(status))
        .add_modifier(Modifier::BOLD);

    // Column border style - highlight if selected
    let border_style = if is_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Line::from(vec![Span::styled(title, status_style)]))
        .title_style(Style::default().add_modifier(Modifier::BOLD));

    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    // Render column border
    f.render_widget(block, area);

    // Get tasks for this column
    let tasks: Vec<_> = app
        .board_nav
        .column_tasks
        .get(column_idx)
        .map(|indices| {
            indices
                .iter()
                .filter_map(|&idx| app.queue.tasks.get(idx).map(|t| (idx, t)))
                .collect()
        })
        .unwrap_or_default();

    // Calculate card height (fixed for simplicity)
    let card_height = 3u16;
    let card_spacing = 1u16;

    // Draw task cards
    for (task_idx_in_column, (_queue_idx, task)) in tasks.iter().enumerate() {
        let card_y = inner.y + (task_idx_in_column as u16 * (card_height + card_spacing));
        if card_y + card_height > inner.y + inner.height {
            // Card would overflow, stop drawing
            break;
        }

        let card_area = Rect {
            x: inner.x,
            y: card_y,
            width: inner.width,
            height: card_height,
        };

        let is_task_selected =
            is_selected && app.board_nav.selected_task_in_column == task_idx_in_column;

        draw_task_card(f, app, card_area, task, is_task_selected);
    }

    // Show "+N more" indicator if there are more tasks than fit
    let visible_cards = (inner.height / (card_height + card_spacing)) as usize;
    if tasks.len() > visible_cards {
        let more_count = tasks.len() - visible_cards;
        let indicator_y = inner.y + inner.height - 1;
        let indicator_area = Rect {
            x: inner.x,
            y: indicator_y,
            width: inner.width,
            height: 1,
        };
        let indicator = Paragraph::new(format!("+{} more", more_count))
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(indicator, indicator_area);
    }
}

/// Draw a single task card.
fn draw_task_card(
    f: &mut Frame<'_>,
    _app: &App,
    area: Rect,
    task: &crate::contracts::Task,
    is_selected: bool,
) {
    // Determine card style based on priority and selection
    let priority = task.priority;
    let priority_fg = priority_color(priority);

    let (border_style, bg_style) = if is_selected {
        (
            Style::default()
                .fg(priority_fg)
                .add_modifier(Modifier::BOLD),
            Style::default().bg(Color::DarkGray),
        )
    } else {
        (Style::default().fg(priority_fg), Style::default())
    };

    // Create card block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });

    // Render card border
    f.render_widget(block, area);

    // Render card content
    let id_span = Span::styled(
        format!("{} ", task.id),
        Style::default()
            .fg(priority_fg)
            .add_modifier(Modifier::BOLD),
    );

    // Truncate title to fit
    let available_width = inner.width.saturating_sub(1) as usize;
    let truncated_title = truncate_chars(&task.title, available_width);
    let title_span = Span::styled(truncated_title, bg_style);

    let line = Line::from(vec![id_span, title_span]);
    let paragraph = Paragraph::new(line);

    f.render_widget(paragraph, inner);
}
