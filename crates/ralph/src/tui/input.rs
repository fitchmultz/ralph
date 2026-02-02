//! Cursor-aware text input model for the TUI.
//!
//! Responsibilities:
//! - Store text input state with a movable cursor (char index).
//! - Provide cursor-aware editing helpers (insert, delete, word delete, movement).
//! - Offer rendering helpers that expose a cursor marker.
//!
//! Not handled here:
//! - Mode transitions, commit/cancel semantics, or key dispatch.
//! - Rendering widgets or layout beyond producing display strings.
//!
//! Invariants/assumptions:
//! - `cursor` is a character index in the range `0..=value.chars().count()`.
//! - Editing operations clamp the cursor to valid bounds.
//! - Input is treated as Unicode scalar values (no grapheme clustering).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput {
    value: String,
    cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextInputEdit {
    Changed,
    Unchanged,
}

impl TextInput {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self { value, cursor }
    }

    pub fn from_parts(value: impl Into<String>, cursor: usize) -> Self {
        let value = value.into();
        let mut input = Self { value, cursor };
        input.clamp_cursor();
        input
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn into_value(self) -> String {
        self.value
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor;
        self.clamp_cursor();
    }

    pub fn move_left(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            return true;
        }
        false
    }

    pub fn move_right(&mut self) -> bool {
        if self.cursor < self.char_len() {
            self.cursor += 1;
            return true;
        }
        false
    }

    pub fn move_home(&mut self) -> bool {
        if self.cursor != 0 {
            self.cursor = 0;
            return true;
        }
        false
    }

    pub fn move_end(&mut self) -> bool {
        let end = self.char_len();
        if self.cursor != end {
            self.cursor = end;
            return true;
        }
        false
    }

    pub fn insert_char(&mut self, ch: char) {
        let mut chars: Vec<char> = self.value.chars().collect();
        let idx = self.cursor.min(chars.len());
        chars.insert(idx, ch);
        self.cursor = idx + 1;
        self.value = chars.into_iter().collect();
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let mut chars: Vec<char> = self.value.chars().collect();
        let idx = self.cursor.min(chars.len());
        if idx == 0 {
            return false;
        }
        chars.remove(idx - 1);
        self.cursor = idx - 1;
        self.value = chars.into_iter().collect();
        true
    }

    pub fn delete(&mut self) -> bool {
        let mut chars: Vec<char> = self.value.chars().collect();
        let idx = self.cursor.min(chars.len());
        if idx >= chars.len() {
            return false;
        }
        chars.remove(idx);
        self.value = chars.into_iter().collect();
        true
    }

    pub fn delete_prev_word(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let mut chars: Vec<char> = self.value.chars().collect();
        let mut idx = self.cursor.min(chars.len());
        if idx == 0 {
            return false;
        }

        let mut removed_whitespace = false;
        while idx > 0 && chars[idx - 1].is_whitespace() {
            idx -= 1;
            removed_whitespace = true;
        }
        if removed_whitespace {
            chars.drain(idx..self.cursor);
            self.cursor = idx;
            self.value = chars.into_iter().collect();
            return true;
        }

        while idx > 0 && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }

        if idx == self.cursor {
            return false;
        }

        chars.drain(idx..self.cursor);
        self.cursor = idx;
        self.value = chars.into_iter().collect();
        true
    }

    pub fn with_cursor_marker(&self, marker: char) -> String {
        let mut output = String::new();
        let mut inserted = false;
        for (idx, ch) in self.value.chars().enumerate() {
            if idx == self.cursor {
                output.push(marker);
                inserted = true;
            }
            output.push(ch);
        }
        if !inserted {
            output.push(marker);
        }
        output
    }

    fn char_len(&self) -> usize {
        self.value.chars().count()
    }

    fn clamp_cursor(&mut self) {
        let max = self.char_len();
        if self.cursor > max {
            self.cursor = max;
        }
    }
}

pub(crate) fn apply_text_input_key(input: &mut TextInput, key: &KeyEvent) -> TextInputEdit {
    let mut changed = false;

    match key.code {
        KeyCode::Left => {
            changed = input.move_left();
        }
        KeyCode::Right => {
            changed = input.move_right();
        }
        KeyCode::Home => {
            changed = input.move_home();
        }
        KeyCode::End => {
            changed = input.move_end();
        }
        KeyCode::Backspace => {
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                changed = input.delete_prev_word();
            } else {
                changed = input.backspace();
            }
        }
        KeyCode::Delete => {
            changed = input.delete();
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            changed = input.delete_prev_word();
        }
        KeyCode::Char(ch) => {
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                input.insert_char(ch);
                changed = true;
            }
        }
        _ => {}
    }

    if changed {
        TextInputEdit::Changed
    } else {
        TextInputEdit::Unchanged
    }
}

#[cfg(test)]
mod tests {
    use super::{TextInput, TextInputEdit, apply_text_input_key};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn insert_and_move_cursor() {
        let mut input = TextInput::new("ab");
        assert_eq!(input.cursor(), 2);

        input.move_left();
        input.insert_char('X');
        assert_eq!(input.value(), "aXb");
        assert_eq!(input.cursor(), 2);

        input.move_home();
        input.insert_char('Y');
        assert_eq!(input.value(), "YaXb");
        assert_eq!(input.cursor(), 1);

        input.move_end();
        input.insert_char('Z');
        assert_eq!(input.value(), "YaXbZ");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn backspace_and_delete() {
        let mut input = TextInput::new("abc");
        input.move_left();
        assert!(input.backspace());
        assert_eq!(input.value(), "ac");
        assert_eq!(input.cursor(), 1);

        assert!(input.delete());
        assert_eq!(input.value(), "a");
        assert_eq!(input.cursor(), 1);

        assert!(!input.delete());
        assert_eq!(input.value(), "a");
    }

    #[test]
    fn delete_prev_word_respects_whitespace() {
        let mut input = TextInput::new("hello   world");
        assert_eq!(input.cursor(), 13);
        assert!(input.delete_prev_word());
        assert_eq!(input.value(), "hello   ");
        assert_eq!(input.cursor(), 8);

        assert!(input.delete_prev_word());
        assert_eq!(input.value(), "hello");
        assert_eq!(input.cursor(), 5);

        assert!(input.delete_prev_word());
        assert_eq!(input.value(), "");
        assert_eq!(input.cursor(), 0);

        assert!(!input.delete_prev_word());
    }

    #[test]
    fn apply_text_input_key_handles_navigation() {
        let mut input = TextInput::new("abc");
        assert_eq!(
            apply_text_input_key(&mut input, &key(KeyCode::Left)),
            TextInputEdit::Changed
        );
        assert_eq!(input.cursor(), 2);
        assert_eq!(
            apply_text_input_key(&mut input, &key(KeyCode::Home)),
            TextInputEdit::Changed
        );
        assert_eq!(input.cursor(), 0);
        assert_eq!(
            apply_text_input_key(&mut input, &key(KeyCode::Home)),
            TextInputEdit::Unchanged
        );
        assert_eq!(
            apply_text_input_key(&mut input, &key(KeyCode::End)),
            TextInputEdit::Changed
        );
        assert_eq!(input.cursor(), 3);
    }

    #[test]
    fn apply_text_input_key_handles_word_delete() {
        let mut input = TextInput::new("alpha beta");
        assert_eq!(
            apply_text_input_key(&mut input, &ctrl_key(KeyCode::Char('w'))),
            TextInputEdit::Changed
        );
        assert_eq!(input.value(), "alpha ");
        assert_eq!(input.cursor(), 6);

        assert_eq!(
            apply_text_input_key(&mut input, &ctrl_key(KeyCode::Backspace)),
            TextInputEdit::Changed
        );
        assert_eq!(input.value(), "alpha");
        assert_eq!(input.cursor(), 5);

        assert_eq!(
            apply_text_input_key(&mut input, &ctrl_key(KeyCode::Backspace)),
            TextInputEdit::Changed
        );
        assert_eq!(input.value(), "");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn with_cursor_marker_inserts_marker() {
        let input = TextInput::from_parts("hello", 2);
        assert_eq!(input.with_cursor_marker('_'), "he_llo");

        let input_end = TextInput::new("hi");
        assert_eq!(input_end.with_cursor_marker('|'), "hi|");
    }
}
