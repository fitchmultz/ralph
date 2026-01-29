//! Tests for TUI render utilities.
//!
//! Responsibilities:
//! - Validate text wrapping, status colors, and priority colors.
//!
//! Not handled here:
//! - Component rendering (see footer.rs, header.rs, etc.).
//! - Overlay or panel rendering.

use crate::contracts::TaskStatus;
use crate::tui::render::utils::{priority_color, status_color, wrap_text};

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
fn wrap_text_handles_zero_width_without_panicking() {
    let lines = wrap_text("hello", 0);
    assert!(!lines.is_empty());
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
        priority_color(crate::contracts::TaskPriority::Critical),
        ratatui::style::Color::Red
    );
    assert_eq!(
        priority_color(crate::contracts::TaskPriority::High),
        ratatui::style::Color::Yellow
    );
    assert_eq!(
        priority_color(crate::contracts::TaskPriority::Medium),
        ratatui::style::Color::Blue
    );
    assert_eq!(
        priority_color(crate::contracts::TaskPriority::Low),
        ratatui::style::Color::DarkGray
    );
}
