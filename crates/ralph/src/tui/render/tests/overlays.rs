//! Tests for overlay rendering.

use std::sync::mpsc;

use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Margin, Rect},
    text::Line,
};

use crate::contracts::QueueFile;
use crate::tui::{App, AppMode, TextInput, app_scroll::ScrollOperations, help};

fn buffer_to_string(buffer: &Buffer) -> String {
    let mut output = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = buffer.cell((x, y)).expect("cell in buffer");
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }
    output
}

fn buffer_line(buffer: &Buffer, x: u16, y: u16, width: u16) -> String {
    let mut output = String::new();
    for col in x..x + width {
        let cell = buffer.cell((col, y)).expect("cell in buffer");
        output.push_str(cell.symbol());
    }
    output
}

fn line_to_string(line: &Line) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

#[test]
fn help_overlay_renders_without_panic() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Help;

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            crate::tui::draw_ui(f, &mut app)
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
    use crate::tui::components::big_text_header::BigTextHeaderComponent;

    let backend = TestBackend::new(70, 12);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Help;

    let area = Rect {
        x: 0,
        y: 0,
        width: 70,
        height: 12,
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
            crate::tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");

    let buffer = terminal.backend().buffer();

    // Calculate header height (same logic as help overlay)
    let header = BigTextHeaderComponent::new("RALPH");
    let header_h = header.measured_height(inner.width).min(inner.height);
    let gap = if inner.height > header_h { 1 } else { 0 };
    let content_y = inner.y + header_h + gap;

    // Check content at the content area (below the header)
    // Note: buffer_line includes trailing spaces, so we trim for comparison
    let content_line = buffer_line(buffer, inner.x, content_y, inner.width);
    assert_eq!(content_line.trim(), expected_top.trim());
}

#[test]
fn command_palette_scrolls_selected_entry_into_view() {
    use ratatui::style::Color;

    let backend = TestBackend::new(80, 8);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CommandPalette {
        query: TextInput::new(""),
        selected: 25,
    };

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            crate::tui::draw_ui(f, &mut app)
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
            crate::tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");
}

#[test]
fn command_palette_renders_cursor_at_position() {
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
            crate::tui::draw_ui(f, &mut app)
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
fn confirm_dialog_overlay_does_not_panic_on_tiny_terminals() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::ConfirmDelete;

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            crate::tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");
}

#[test]
fn confirm_revert_overlay_renders_preface_before_prompt() {
    let backend = TestBackend::new(80, 16);
    let mut terminal = Terminal::new(backend).expect("create terminal");
    let mut app = App::new(QueueFile::default());
    let (tx, _rx) = mpsc::channel();
    let preface = "Scan validation failed after run.\n(raw stdout saved to /tmp/output.txt)";

    app.mode = AppMode::ConfirmRevert {
        label: "Scan validation failure".to_string(),
        preface: Some(preface.to_string()),
        allow_proceed: false,
        selected: 0,
        input: TextInput::new(""),
        reply_sender: tx,
        previous_mode: Box::new(AppMode::Normal),
    };

    terminal
        .draw(|f| {
            app.detail_width = f.area().width.saturating_sub(4);
            crate::tui::draw_ui(f, &mut app)
        })
        .expect("draw ui");

    let buffer = terminal.backend().buffer();
    let output = buffer_to_string(buffer);
    let lines: Vec<&str> = output.lines().collect();
    let preface_row = lines
        .iter()
        .position(|line| line.contains("Scan validation failed after run."))
        .expect("preface line");
    let prompt_row = lines
        .iter()
        .position(|line| line.contains("Scan validation failure: action?"))
        .expect("prompt line");

    assert!(
        preface_row < prompt_row,
        "expected preface above prompt, got: {output:?}"
    );
}
