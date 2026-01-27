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

use super::super::{App, AppMode};
use super::utils::{priority_color, status_color, wrap_text};
use crate::contracts::{QueueFile, TaskPriority, TaskStatus};
use crate::tui;
use ratatui::text::Span;

fn spans_to_string(spans: &[Span<'static>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

fn footer_text(app: &App, width: usize) -> String {
    spans_to_string(&super::footer::help_footer_spans(app, width))
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
    let lines = super::overlays::help_overlay_plain_lines();
    let rendered = lines.join("\n");

    for expected in [
        "K/J: move selected task up/down",
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
        query: "".to_string(),
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
fn command_palette_overlay_does_not_panic_on_tiny_terminals() {
    use ratatui::{backend::TestBackend, Terminal};

    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CommandPalette {
        query: "".to_string(),
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
