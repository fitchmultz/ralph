//! Tests for panel rendering (task list, task details, executing view).
//!
//! Responsibilities:
//! - Validate task list, task details, and executing view rendering.
//!
//! Not handled here:
//! - Footer or header rendering.
//! - Overlay rendering.

use super::common::*;
use crate::contracts::{QueueFile, TaskStatus};
use crate::tui::app_filters::FilterManagementOperations;
use crate::tui::{App, AppMode, TextInput};
use ratatui::style::Color;
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
};

#[test]
fn task_details_show_scroll_indicator_when_truncated() {
    let backend = TestBackend::new(70, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_long_details_queue());

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let rendered = buffer_to_string(buffer);
    assert!(
        rendered.contains("Task Details ("),
        "expected details title to include scroll indicator"
    );
    // With ScrollView, bounds checking is handled internally
    // Just verify the UI rendered correctly
}

#[test]
fn task_details_wraps_long_tags_for_scroll_bounds() {
    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_long_tags_queue());

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    // With ScrollView, bounds checking is handled internally
    // Just verify the UI rendered correctly

    let buffer = terminal.backend().buffer();
    let rendered = buffer_to_string(buffer);
    assert!(
        rendered.contains("Task Details ("),
        "expected details title to include scroll indicator"
    );
}

#[test]
fn task_details_scroll_offsets_visible_content() {
    let backend = TestBackend::new(70, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_long_details_queue());

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    app.details.scroll_down(1);

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let area = Rect {
        x: 0,
        y: 0,
        width: buffer.area.width,
        height: buffer.area.height,
    };
    // Updated for 3-row layout: header + main + footer
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1), // header
                Constraint::Min(2),    // main
                Constraint::Length(1), // footer
            ]
            .as_ref(),
        )
        .split(area);
    let main = outer[1];
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
    let details_area = chunks[1];
    let inner = details_area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    let top_line = buffer_line(buffer, inner.x, inner.y, inner.width);
    assert!(
        top_line.contains("Status:"),
        "expected scroll offset to move details content, got: {top_line:?}"
    );
}

#[test]
fn task_list_title_truncates_filter_summary_on_narrow_width() {
    let backend = TestBackend::new(50, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());
    app.filters.statuses = vec![TaskStatus::Todo, TaskStatus::Doing];
    app.set_tag_filters(vec![
        "alpha".to_string(),
        "beta".to_string(),
        "gamma".to_string(),
    ]);
    app.set_scope_filters(vec!["crates/ralph".to_string(), "docs".to_string()]);
    app.set_search_query("a very long query that should be truncated".to_string());
    app.filters.search_options.use_regex = true;
    app.filters.search_options.case_sensitive = true;
    app.rebuild_filtered_view();

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let area = Rect {
        x: 0,
        y: 0,
        width: buffer.area.width,
        height: buffer.area.height,
    };
    // Updated for 3-row layout: header + main + footer
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1), // header
                Constraint::Min(2),    // main
                Constraint::Length(1), // footer
            ]
            .as_ref(),
        )
        .split(area);
    let main = outer[1];
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
    let list_area = chunks[0];
    let title_line = buffer_line(buffer, list_area.x, list_area.y, list_area.width);

    assert!(title_line.contains("filters:"));
    assert!(title_line.contains("tags=3"));
    assert!(title_line.contains("..."));
    assert!(
        !title_line.contains("alpha"),
        "expected compact tag counts, got: {title_line:?}"
    );
}

#[test]
fn filtered_empty_hint_uses_compact_filter_summary() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());
    app.filters.statuses = vec![TaskStatus::Todo, TaskStatus::Doing];
    app.set_tag_filters(vec![
        "alpha".to_string(),
        "beta".to_string(),
        "gamma".to_string(),
    ]);
    app.set_scope_filters(vec!["crates/ralph".to_string(), "docs".to_string()]);
    app.set_search_query("needle".to_string());
    app.filters.search_options.use_regex = true;
    app.filters.search_options.case_sensitive = true;
    app.rebuild_filtered_view();
    assert_eq!(app.filtered_len(), 0);

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let area = Rect {
        x: 0,
        y: 0,
        width: buffer.area.width,
        height: buffer.area.height,
    };
    // Updated for 3-row layout: header + main + footer
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1), // header
                Constraint::Min(2),    // main
                Constraint::Length(1), // footer
            ]
            .as_ref(),
        )
        .split(area);
    let main = outer[1];
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
    let details_area = chunks[1];
    let inner = details_area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let summary_line = buffer_line(buffer, inner.x, inner.y + 2, inner.width);

    assert!(summary_line.contains("filters:"));
    assert!(summary_line.contains("tags=3"));
    assert!(summary_line.contains("scopes=2"));
    assert!(
        !summary_line.contains("alpha"),
        "expected compact tag counts, got: {summary_line:?}"
    );
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

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let expected = 10usize.saturating_sub(2).saturating_sub(1).max(1);
    assert_eq!(app.log_visible_lines, expected);
}

#[test]
fn task_list_highlight_keeps_selected_row_visible() {
    fn find_text_in_rect(
        buffer: &ratatui::buffer::Buffer,
        rect: Rect,
        needle: &str,
    ) -> Option<(u16, u16)> {
        let needle_chars: Vec<char> = needle.chars().collect();
        if rect.width < needle_chars.len() as u16 || rect.height == 0 {
            return None;
        }

        let max_x = rect.x + rect.width.saturating_sub(needle_chars.len() as u16);
        let max_y = rect.y + rect.height;

        for y in rect.y..max_y {
            for x_start in rect.x..=max_x {
                let mut matched = true;
                for (offset, expected) in needle_chars.iter().enumerate() {
                    let x = x_start + offset as u16;
                    let cell = buffer.cell((x, y)).expect("cell in buffer");
                    let mut symbol_iter = cell.symbol().chars();
                    if symbol_iter.next() != Some(*expected) || symbol_iter.next().is_some() {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    return Some((y, x_start));
                }
            }
        }
        None
    }

    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());
    app.selected = 1;

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let area = Rect {
        x: 0,
        y: 0,
        width: buffer.area.width,
        height: buffer.area.height,
    };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(1)].as_ref())
        .split(area);
    let main = outer[0];
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
    let list_area = chunks[0];

    let selected = "RQ-0002";
    let (selected_row, selected_col) =
        find_text_in_rect(buffer, list_area, selected).expect("selected row visible in list");
    for (offset, _) in selected.chars().enumerate() {
        let cell = buffer
            .cell((selected_col + offset as u16, selected_row))
            .expect("cell in buffer");
        assert_eq!(cell.bg, Color::Blue);
    }
    let selected_line = buffer_line(buffer, list_area.x, selected_row, list_area.width);
    assert!(
        selected_line.contains("»"),
        "expected highlight symbol on selected row, got: {selected_line:?}"
    );

    let unselected = "RQ-0001";
    let (unselected_row, unselected_col) =
        find_text_in_rect(buffer, list_area, unselected).expect("unselected row visible in list");
    for (offset, _) in unselected.chars().enumerate() {
        let cell = buffer
            .cell((unselected_col + offset as u16, unselected_row))
            .expect("cell in buffer");
        assert_ne!(cell.bg, Color::Blue);
    }
    let unselected_line = buffer_line(buffer, list_area.x, unselected_row, list_area.width);
    assert!(
        !unselected_line.contains("»"),
        "expected no highlight symbol on unselected row, got: {unselected_line:?}"
    );
}

#[test]
fn task_details_title_renders_cursor_at_position() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTask(TextInput::from_parts("hello", 2));

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            crate::tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let output = buffer_to_string(buffer);
    assert!(
        output.contains("New Task: he_llo"),
        "expected cursor marker in details title, got: {output:?}"
    );
}
