//! Unit tests for TUI rendering components.
//!
//! These are isolated unit tests for rendering utilities and helpers.

use super::super::{App, AppMode};
use super::utils::{priority_color, status_color, wrap_text};
use crate::contracts::{QueueFile, TaskPriority, TaskStatus};
use crate::tui;

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

    let help_text = super::footer::help_footer_spans(&app);
    let rendered = format!("{:?}", help_text);

    assert!(rendered.contains("SAVE ERROR"));
}

#[test]
fn help_footer_includes_config_hint() {
    let app = App::new(QueueFile::default());
    let help_text = super::footer::help_footer_spans(&app);
    let rendered = format!("{:?}", help_text);

    assert!(rendered.contains(":config"));
}

#[test]
fn help_footer_includes_scan_hint() {
    let app = App::new(QueueFile::default());
    let help_text = super::footer::help_footer_spans(&app);
    let rendered = format!("{:?}", help_text);

    assert!(rendered.contains(":scan"));
}

#[test]
fn help_footer_excludes_save_error_when_none() {
    let mut app = App::new(QueueFile::default());
    app.save_error = None;

    let help_text = super::footer::help_footer_spans(&app);
    let rendered = format!("{:?}", help_text);

    assert!(!rendered.contains("SAVE ERROR"));
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
