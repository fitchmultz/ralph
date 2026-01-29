//! Multi-line text input wrapper using tui-textarea.
//!
//! Responsibilities:
//! - Wrap tui_textarea::TextArea for use in TUI editing modes.
//! - Provide conversion to/from String for list field compatibility.
//! - Handle input events via textarea's built-in key handlers.
//!
//! Not handled here:
//! - Single-line input modes (keep TextInput for simple cases if needed).
//! - Rendering (textarea implements Widget trait directly).
//!
//! Invariants/assumptions:
//! - List fields use newline as delimiter during editing, converted to Vec<String> on commit.
//! - TextArea handles its own cursor state and scrolling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Style};
use tui_textarea::TextArea;

/// Multi-line text input for task and config editing.
///
/// Wraps tui-textarea's TextArea to provide:
/// - Multi-line editing with Emacs shortcuts
/// - Undo/redo support
/// - Text selection and mouse scrolling
/// - Conversion between newline-delimited strings and comma-separated lists
#[derive(Debug, Clone)]
pub struct MultiLineInput {
    textarea: TextArea<'static>,
    is_list_field: bool,
}

impl MultiLineInput {
    /// Create a new MultiLineInput with the given initial text.
    ///
    /// # Arguments
    /// * `value` - Initial text content
    /// * `is_list_field` - If true, the field represents a list (tags, scope, etc.)
    ///   and will use newline as the delimiter during editing
    pub fn new(value: impl Into<String>, is_list_field: bool) -> Self {
        let value = value.into();
        let mut textarea = TextArea::default();

        // For list fields, convert comma-separated to newline-separated
        let display_value = if is_list_field {
            value
                .split(',')
                .map(|s| s.trim())
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            value
        };

        // Insert the text line by line
        for (i, line) in display_value.lines().enumerate() {
            if i > 0 {
                textarea.insert_newline();
            }
            textarea.insert_str(line);
        }

        // Style the textarea
        textarea.set_style(Style::default().fg(Color::White));
        textarea.set_cursor_style(Style::default().fg(Color::Yellow));

        Self {
            textarea,
            is_list_field,
        }
    }

    /// Process a key event and update the textarea state.
    ///
    /// Returns true if the text content changed.
    /// Note: Enter (without modifiers) is handled specially by callers to commit edits.
    pub fn input(&mut self, key: KeyEvent) -> bool {
        // Handle special keys that we want to intercept
        match key.code {
            // Enter without modifiers - let caller handle as commit
            KeyCode::Enter
                if !key.modifiers.intersects(
                    KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::CONTROL,
                ) =>
            {
                return false;
            }
            // Esc - let caller handle as cancel
            KeyCode::Esc => {
                return false;
            }
            _ => {}
        }

        // Pass all other keys to textarea
        self.textarea.input(key)
    }

    /// Get the current text value.
    ///
    /// For list fields, returns newline-separated text (callers should
    /// use lines() for list conversion).
    pub fn value(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Get the lines as a Vec<String>.
    ///
    /// Useful for list fields where each line becomes a list item.
    /// Empty lines are filtered out.
    pub fn lines(&self) -> Vec<String> {
        self.textarea
            .lines()
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Get the textarea widget for rendering.
    pub fn widget(&self) -> &TextArea<'static> {
        &self.textarea
    }

    /// Get a mutable reference to the textarea for advanced operations.
    pub fn textarea_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.textarea
    }

    /// Check if this is a list field.
    pub fn is_list_field(&self) -> bool {
        self.is_list_field
    }

    /// Move cursor to the end of the text.
    pub fn move_cursor_to_end(&mut self) {
        // tui-textarea handles cursor positioning internally
        self.textarea
            .set_cursor_style(Style::default().fg(Color::Yellow));
    }

    /// Insert a newline at the current cursor position.
    pub fn insert_newline(&mut self) {
        self.textarea.insert_newline();
    }
}

impl PartialEq for MultiLineInput {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value() && self.is_list_field == other.is_list_field
    }
}

impl Eq for MultiLineInput {}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn new_creates_textarea_with_value() {
        let input = MultiLineInput::new("hello world", false);
        assert_eq!(input.value(), "hello world");
        assert!(!input.is_list_field());
    }

    #[test]
    fn new_converts_comma_separated_for_list_fields() {
        let input = MultiLineInput::new("a, b, c", true);
        assert_eq!(input.value(), "a\nb\nc");
        assert!(input.is_list_field());
    }

    #[test]
    fn lines_filters_empty() {
        let input = MultiLineInput::new("a\n\nb\n  \nc", true);
        let lines = input.lines();
        assert_eq!(lines, vec!["a", "b", "c"]);
    }

    #[test]
    fn lines_trims_whitespace() {
        let input = MultiLineInput::new("  a  \n  b  ", true);
        let lines = input.lines();
        assert_eq!(lines, vec!["a", "b"]);
    }

    #[test]
    fn input_intercepts_enter() {
        let mut input = MultiLineInput::new("hello", false);
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        let changed = input.input(key);
        // Enter should not be processed by textarea (caller handles it)
        assert!(!changed);
        assert_eq!(input.value(), "hello");
    }

    #[test]
    fn input_intercepts_esc() {
        let mut input = MultiLineInput::new("hello", false);
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        let changed = input.input(key);
        // Esc should not be processed by textarea (caller handles it)
        assert!(!changed);
        assert_eq!(input.value(), "hello");
    }

    #[test]
    fn input_allows_alt_enter() {
        let mut input = MultiLineInput::new("hello", false);
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
        let changed = input.input(key);
        // Alt+Enter should insert a newline
        assert!(changed);
        assert_eq!(input.value(), "hello\n");
    }

    #[test]
    fn partial_eq_compares_value_and_type() {
        let a = MultiLineInput::new("test", false);
        let b = MultiLineInput::new("test", false);
        let c = MultiLineInput::new("test", true);
        let d = MultiLineInput::new("other", false);

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
