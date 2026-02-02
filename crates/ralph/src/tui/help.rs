//! Shared help overlay content for the TUI.
//!
//! Responsibilities:
//! - Build the help overlay line list from the canonical keymap.
//! - Provide wrapped line counts for scrolling/rendering.
//!
//! Not handled here:
//! - Rendering styles or layout beyond line construction.
//! - Event handling for help navigation.
//!
//! Invariants/assumptions:
//! - Keymap sections are the source of truth for help content.
//! - Callers provide a non-zero width when requesting wrapped lines.

use super::keymap;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone)]
enum HelpLine {
    Title(&'static str),
    Section(&'static str),
    Text(String),
    Blank,
}

pub(crate) fn help_overlay_lines(width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut rendered = Vec::new();
    let bold = Style::default().add_modifier(Modifier::BOLD);

    for line in build_help_overlay_lines() {
        match line {
            HelpLine::Title(text) | HelpLine::Section(text) => {
                push_wrapped_lines(&mut rendered, text, bold, width);
            }
            HelpLine::Text(text) => {
                push_wrapped_lines(&mut rendered, &text, Style::default(), width);
            }
            HelpLine::Blank => rendered.push(Line::from("")),
        }
    }

    rendered
}

pub(crate) fn help_line_count(width: usize) -> usize {
    help_overlay_lines(width).len()
}

#[cfg(test)]
pub(crate) fn help_overlay_plain_lines() -> Vec<String> {
    build_help_overlay_lines()
        .into_iter()
        .map(|line| match line {
            HelpLine::Title(text) | HelpLine::Section(text) => text.to_string(),
            HelpLine::Text(text) => text,
            HelpLine::Blank => String::new(),
        })
        .collect()
}

fn build_help_overlay_lines() -> Vec<HelpLine> {
    let mut lines = Vec::new();
    lines.push(HelpLine::Title("Keybindings"));
    let close_keys = join_keys(keymap::help_close_keys());
    lines.push(HelpLine::Text(format!("Press {close_keys} to close.")));
    lines.push(HelpLine::Blank);

    for section in keymap::help_sections() {
        push_section_lines(&mut lines, section);
    }

    for section in keymap::normal_sections() {
        push_section_lines(&mut lines, section);
    }

    // Add board navigation section
    for section in keymap::board_sections() {
        push_section_lines(&mut lines, section);
    }

    // Add multi-select section
    for section in keymap::multi_select_sections() {
        push_section_lines(&mut lines, section);
    }

    for section in keymap::executing_sections() {
        push_section_lines(&mut lines, section);
    }

    while matches!(lines.last(), Some(HelpLine::Blank)) {
        lines.pop();
    }

    lines
}

fn push_section_lines(lines: &mut Vec<HelpLine>, section: &keymap::KeymapSection) {
    lines.push(HelpLine::Section(section.title));
    for binding in section.bindings {
        lines.push(HelpLine::Text(format!(
            "{}: {}",
            binding.keys_display, binding.description
        )));
    }
    lines.push(HelpLine::Blank);
}

fn join_keys(keys: &[&str]) -> String {
    match keys.len() {
        0 => String::new(),
        1 => keys[0].to_string(),
        2 => format!("{} or {}", keys[0], keys[1]),
        _ => {
            let mut out = String::new();
            for (idx, key) in keys.iter().enumerate() {
                if idx > 0 {
                    if idx == keys.len() - 1 {
                        out.push_str(" or ");
                    } else {
                        out.push_str(", ");
                    }
                }
                out.push_str(key);
            }
            out
        }
    }
}

fn push_wrapped_lines(out: &mut Vec<Line<'static>>, text: &str, style: Style, width: usize) {
    let width = width.max(1);
    for wrapped in textwrap::wrap(text, width) {
        out.push(Line::from(Span::styled(wrapped.into_owned(), style)));
    }
}
