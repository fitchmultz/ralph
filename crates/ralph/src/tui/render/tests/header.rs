//! Tests for header/status bar rendering.
//!
//! Responsibilities:
//! - Validate header content including mode, dirty indicators, runner status, etc.
//!
//! Not handled here:
//! - Footer or panel rendering.
//! - Overlay rendering.

use super::common::*;
use crate::contracts::{QueueFile, TaskStatus};
use crate::tui::{App, AppMode, TextInput};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn header_shows_mode_normal() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        header_line.contains("Normal"),
        "expected header to show Normal mode, got: {header_line:?}"
    );
}

#[test]
fn header_shows_dirty_indicators() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.dirty = true;
    app.dirty_done = true;

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        header_line.contains("*queue"),
        "expected header to show *queue dirty indicator, got: {header_line:?}"
    );
    assert!(
        header_line.contains("*done"),
        "expected header to show *done dirty indicator, got: {header_line:?}"
    );
}

#[test]
fn header_shows_runner_status() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.runner_active = true;
    app.running_task_id = Some("RQ-0001".to_string());

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        header_line.contains("RQ-0001"),
        "expected header to show running task ID, got: {header_line:?}"
    );
}

#[test]
fn header_shows_loop_status() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.loop_active = true;
    app.loop_ran = 3;
    app.loop_max_tasks = Some(10);

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        header_line.contains("3/10"),
        "expected header to show loop progress, got: {header_line:?}"
    );
}

#[test]
fn header_shows_task_count() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    // Should show task count (3 tasks in make_task_list_queue)
    assert!(
        header_line.contains('3'),
        "expected header to show task count, got: {header_line:?}"
    );
}

#[test]
fn header_truncates_narrow_terminal() {
    let backend = TestBackend::new(30, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());
    app.dirty = true;
    app.runner_active = true;
    app.running_task_id = Some("RQ-0001".to_string());

    // Should not panic on narrow terminal
    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    // Mode should still be visible
    assert!(
        header_line.contains("Normal"),
        "expected mode to be visible even on narrow terminal, got: {header_line:?}"
    );
}

#[test]
fn header_shows_filter_summary() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(make_task_list_queue());
    app.filters.statuses = vec![TaskStatus::Todo, TaskStatus::Doing];
    app.rebuild_filtered_view();

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        header_line.contains("status=2"),
        "expected header to show filter summary, got: {header_line:?}"
    );
}

#[test]
fn header_shows_mode_creating_task() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTask(TextInput::new(""));

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        header_line.contains("Creating Task"),
        "expected header to show Creating Task mode, got: {header_line:?}"
    );
}

#[test]
fn header_shows_mode_help() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Help;

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        header_line.contains("Help"),
        "expected header to show Help mode, got: {header_line:?}"
    );
}

#[test]
fn header_does_not_show_in_executing_view() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.running_task_id = Some("RQ-0001".to_string());

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    // In executing view, the first line should show "Executing:" not "[Normal]"
    let first_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        first_line.contains("Executing"),
        "expected executing view title, got: {first_line:?}"
    );
}

#[test]
fn header_shows_mode_flowchart() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::FlowchartOverlay {
        previous_mode: Box::new(AppMode::Normal),
    };

    terminal
        .draw(|f| crate::tui::draw_ui(f, &mut app))
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let header_line = buffer_line(buffer, 0, 0, buffer.area.width);
    assert!(
        header_line.contains("Flowchart"),
        "expected header to show Flowchart mode, got: {header_line:?}"
    );
}
