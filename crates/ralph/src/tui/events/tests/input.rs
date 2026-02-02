//! Tests for basic input handling and text character processing.
//!
//! Responsibilities:
//! - Test basic input parsing and character handling.
//! - Validate modifier key behavior.
//!
//! Does NOT handle:
//! - Mode-specific input handling (see mode-specific test modules).
//! - Complex input sequences or gestures.

use crate::tui::events::text_char;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn text_char_ignores_ctrl_and_alt_modifiers() {
    let plain = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(text_char(&plain), Some('a'));

    let shifted = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
    assert_eq!(text_char(&shifted), Some('A'));

    let ctrl = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(text_char(&ctrl), None);

    let alt = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
    assert_eq!(text_char(&alt), None);
}
