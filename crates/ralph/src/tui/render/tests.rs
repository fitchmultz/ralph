//! Unit tests for TUI rendering components.
//!
//! Responsibilities:
//! - Validate rendering helpers and overlays using `TestBackend`.
//! - Confirm rendered output for key overlays and layout utilities.
//!
//! Not handled here:
//! - Event handling or queue mutation logic.
//! - Runner execution side effects or terminal IO integration.
//!
//! Invariants/assumptions:
//! - Tests use deterministic buffers and ASCII-only assertions.

use super::super::{help, App, AppMode, TextInput};
use super::utils::{priority_color, status_color, wrap_text};
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::tui;
use ratatui::text::Span;
use ratatui::{buffer::Buffer, layout::Margin};
use std::collections::HashMap;

fn spans_to_string(spans: &[Span<'static>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

fn footer_text(app: &App, width: usize) -> String {
    spans_to_string(&super::footer::help_footer_spans(app, width))
}

fn buffer_to_string(buffer: &Buffer) -> String {
    let mut lines = Vec::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            let cell = buffer.cell((x, y)).expect("cell in buffer");
            line.push_str(cell.symbol());
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn buffer_line(buffer: &Buffer, x: u16, y: u16, width: u16) -> String {
    let mut line = String::new();
    for offset in 0..width {
        let cell = buffer.cell((x + offset, y)).expect("cell in buffer");
        line.push_str(cell.symbol());
    }
    line.trim_end().to_string()
}

fn line_to_string(line: &ratatui::text::Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn make_long_details_queue() -> QueueFile {
    let evidence: Vec<String> = (0..20).map(|i| format!("Evidence line {i}")).collect();
    let plan: Vec<String> = (0..10).map(|i| format!("Plan step {i}")).collect();
    QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0001".to_string(),
            title: "Long Task".to_string(),
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence,
            plan,
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: vec![],
            custom_fields: HashMap::new(),
        }],
    }
}

fn make_long_tags_queue() -> QueueFile {
    let tags: Vec<String> = (0..40).map(|i| format!("very-long-tag-{i:02}")).collect();
    QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0002".to_string(),
            title: "Tagged Task".to_string(),
            status: TaskStatus::Todo,
            priority: TaskPriority::Low,
            tags,
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: vec![],
            custom_fields: HashMap::new(),
        }],
    }
}

fn make_task_list_queue() -> QueueFile {
    let make_task = |id: &str, title: &str, status: TaskStatus| Task {
        id: id.to_string(),
        title: title.to_string(),
        status,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at: None,
        depends_on: vec![],
        custom_fields: HashMap::new(),
    };

    QueueFile {
        version: 1,
        tasks: vec![
            make_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}

#[test]
fn wrap_text_returns_nonempty_for_nonempty_input() {
    let lines = wrap_text("hello world", 5);
    assert!(!lines.is_empty());
    assert!(lines
        .iter()
        .any(|l| l.contains("hello") || l.contains("world")));
}

#[test]
fn wrap_text_splits_long_lines() {
    let lines = wrap_text("a very long line that should be split", 10);
    assert!(lines.len() > 1);
    for line in &lines {
        assert!(line.len() <= 10);
    }
}

#[test]
fn status_color_maps_all_statuses() {
    assert_eq!(
        status_color(TaskStatus::Draft),
        ratatui::style::Color::DarkGray
    );
    assert_eq!(status_color(TaskStatus::Todo), ratatui::style::Color::Blue);
    assert_eq!(
        status_color(TaskStatus::Doing),
        ratatui::style::Color::Yellow
    );
    assert_eq!(status_color(TaskStatus::Done), ratatui::style::Color::Green);
    assert_eq!(
        status_color(TaskStatus::Rejected),
        ratatui::style::Color::Red
    );
}

#[test]
fn priority_color_maps_all_priorities() {
    assert_eq!(
        priority_color(TaskPriority::Critical),
        ratatui::style::Color::Red
    );
    assert_eq!(
        priority_color(TaskPriority::High),
        ratatui::style::Color::Yellow
    );
    assert_eq!(
        priority_color(TaskPriority::Medium),
        ratatui::style::Color::Blue
    );
    assert_eq!(
        priority_color(TaskPriority::Low),
        ratatui::style::Color::DarkGray
    );
}

#[test]
fn help_footer_includes_save_error_indicator() {
    let mut app = App::new(QueueFile::default());
    app.save_error = Some("failed to save".to_string());

    let rendered = footer_text(&app, 160);

    assert!(rendered.contains("SAVE ERROR"));
}

#[test]
fn help_footer_includes_config_hint() {
    let app = App::new(QueueFile::default());
    let rendered = footer_text(&app, 160);

    assert!(rendered.contains(":config"));
}

#[test]
fn help_footer_includes_scan_hint() {
    let app = App::new(QueueFile::default());
    let rendered = footer_text(&app, 160);

    assert!(rendered.contains(":scan"));
}

#[test]
fn help_footer_excludes_save_error_when_none() {
    let mut app = App::new(QueueFile::default());
    app.save_error = None;

    let rendered = footer_text(&app, 160);

    assert!(!rendered.contains("SAVE ERROR"));
}

#[test]
fn help_footer_includes_keymap_shortcuts_in_normal_mode() {
    let app = App::new(QueueFile::default());
    let rendered = footer_text(&app, 240);

    for expected in ["K/J", "Ctrl+P", "Ctrl+F", ":scope", ":case", ":regex"] {
        assert!(
            rendered.contains(expected),
            "missing footer hint: {expected}"
        );
    }
}

#[test]
fn help_footer_truncates_with_ellipsis_on_small_width() {
    let app = App::new(QueueFile::default());
    let rendered = footer_text(&app, 12);

    assert!(rendered.contains("..."));
}

#[test]
fn help_overlay_includes_keymap_shortcuts() {
    let lines = help::help_overlay_plain_lines();
    let rendered = lines.join("\n");

    for expected in [
        "K/J: move selected task up/down",
        "Tab/Shift+Tab: switch focus between list/details",
        "PgUp/PgDn: page list/details (focused panel)",
        "C: toggle case-sensitive search",
        "R: toggle regex search",
        "Ctrl+P: command palette (shortcut)",
        "Ctrl+F: search tasks (shortcut)",
        "o: filter by scope",
    ] {
        assert!(
            rendered.contains(expected),
            "help overlay missing: {expected}"
        );
    }
}

#[test]
fn help_overlay_shows_scroll_indicator_when_truncated() {
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Help;

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let rendered = buffer_to_string(buffer);
    assert!(
        rendered.contains("Help ("),
        "expected help title to include scroll indicator"
    );

    let area = Rect {
        x: 0,
        y: 0,
        width: buffer.area.width,
        height: buffer.area.height,
    };
    let popup = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let inner = popup.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let total_lines = help::help_line_count(inner.width as usize);
    assert!(
        total_lines > inner.height as usize,
        "expected help content to exceed visible height"
    );
}

#[test]
fn help_overlay_scroll_offsets_visible_content() {
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    let backend = TestBackend::new(70, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Help;

    let area = Rect {
        x: 0,
        y: 0,
        width: 70,
        height: 10,
    };
    let popup = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let inner = popup.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    let lines = help::help_overlay_lines(inner.width as usize);
    let scroll_offset = 3usize;
    let expected_top = line_to_string(&lines[scroll_offset]);

    app.set_help_visible_lines(inner.height as usize, lines.len());
    app.scroll_help_down(scroll_offset, lines.len());

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let top_line = buffer_line(buffer, inner.x, inner.y, inner.width);
    assert_eq!(top_line, expected_top);
}

#[test]
fn task_details_show_scroll_indicator_when_truncated() {
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(70, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_long_details_queue());

    terminal
        .draw(|f| tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let rendered = buffer_to_string(buffer);
    assert!(
        rendered.contains("Task Details ("),
        "expected details title to include scroll indicator"
    );
    assert!(
        app.details_total_lines > app.details_visible_lines,
        "expected details content to exceed visible height"
    );
}

#[test]
fn task_details_wraps_long_tags_for_scroll_bounds() {
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_long_tags_queue());

    terminal
        .draw(|f| tui::draw_ui(f, &mut app))
        .expect("draw ui");

    assert!(
        app.details_total_lines > app.details_visible_lines,
        "expected wrapped tags to exceed visible height"
    );

    let buffer = terminal.backend().buffer();
    let rendered = buffer_to_string(buffer);
    assert!(
        rendered.contains("Task Details ("),
        "expected details title to include scroll indicator"
    );
}

#[test]
fn task_details_scroll_offsets_visible_content() {
    use ratatui::{
        backend::TestBackend,
        layout::{Constraint, Direction, Layout, Rect},
        Terminal,
    };

    let backend = TestBackend::new(70, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_long_details_queue());

    terminal
        .draw(|f| tui::draw_ui(f, &mut app))
        .expect("draw ui");

    app.details_scroll = 1;

    terminal
        .draw(|f| tui::draw_ui(f, &mut app))
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
fn executing_view_updates_visible_lines_cache() {
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.running_task_id = Some("RQ-0001".to_string());
    app.log_visible_lines = 20;

    terminal
        .draw(|f| tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let expected = 10usize.saturating_sub(2).saturating_sub(1).max(1);
    assert_eq!(app.log_visible_lines, expected);
}

#[test]
fn wrap_text_handles_zero_width_without_panicking() {
    let lines = wrap_text("hello", 0);
    assert!(!lines.is_empty());
}

#[test]
fn command_palette_scrolls_selected_entry_into_view() {
    use ratatui::style::Color;
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(80, 8);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CommandPalette {
        query: TextInput::new(""),
        selected: 16,
    };

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let target = "Toggle regex search";
    let target_chars: Vec<char> = target.chars().collect();
    let target_len = target_chars.len() as u16;
    let mut found = None;

    assert!(
        buffer.area.width >= target_len,
        "terminal width too small for target"
    );

    for y in 0..buffer.area.height {
        for x_start in 0..=buffer.area.width.saturating_sub(target_len) {
            let mut matched = true;
            for (offset, expected) in target_chars.iter().enumerate() {
                let x = x_start + offset as u16;
                let cell = buffer.cell((x, y)).expect("cell in buffer");
                let mut symbol_iter = cell.symbol().chars();
                if symbol_iter.next() != Some(*expected) || symbol_iter.next().is_some() {
                    matched = false;
                    break;
                }
            }
            if matched {
                found = Some((y, x_start));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }

    let (row, col) = found.expect("expected selected entry to be visible");
    for offset in 0..target_chars.len() {
        let cell = buffer
            .cell((col + offset as u16, row))
            .expect("cell in buffer");
        assert_eq!(cell.bg, Color::Blue);
    }
}

#[test]
fn task_list_highlight_keeps_selected_row_visible() {
    use ratatui::style::Color;
    use ratatui::{
        backend::TestBackend,
        layout::{Constraint, Direction, Layout, Rect},
        Terminal,
    };

    fn find_text_in_rect(buffer: &Buffer, rect: Rect, needle: &str) -> Option<(u16, u16)> {
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
        .draw(|f| tui::draw_ui(f, &mut app))
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
fn command_palette_overlay_does_not_panic_on_tiny_terminals() {
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CommandPalette {
        query: TextInput::new(""),
        selected: 0,
    };

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");
}

#[test]
fn command_palette_renders_cursor_at_position() {
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CommandPalette {
        query: TextInput::from_parts("run", 1),
        selected: 0,
    };

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let output = buffer_to_string(buffer);
    assert!(
        output.contains(":r_un"),
        "expected cursor marker in command palette input, got: {output:?}"
    );
}

#[test]
fn task_details_title_renders_cursor_at_position() {
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTask(TextInput::from_parts("hello", 2));

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let output = buffer_to_string(buffer);
    assert!(
        output.contains("New Task: he_llo"),
        "expected cursor marker in details title, got: {output:?}"
    );
}

#[test]
fn confirm_dialog_overlay_does_not_panic_on_tiny_terminals() {
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::ConfirmDelete;

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");
}
