//! Event types for the Ralph TUI foundation.
//!
//! Responsibilities:
//! - Define a unified event type that wraps crossterm events for component handling.
//! - Provide a common interface for key and mouse events.
//!
//! Not handled here:
//! - Event dispatch or routing (handled by the app event loop).
//! - Mode-specific keybindings (handled by mode-specific event handlers).
//!
//! Invariants/assumptions:
//! - Events are converted from crossterm events before being passed to components.
//! - Components can check event types without importing crossterm directly.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// Unified UI event type for component handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiEvent {
    /// Keyboard event.
    Key(KeyEvent),
    /// Mouse event.
    Mouse(MouseEvent),
    /// Terminal resize event (new width, new height).
    Resize(u16, u16),
    /// Focus gained event.
    FocusGained,
    /// Focus lost event.
    FocusLost,
    /// Paste event (contains the pasted text).
    Paste(String),
}

impl UiEvent {
    /// Create a UI event from a crossterm event.
    pub(crate) fn from_crossterm(event: crossterm::event::Event) -> Option<Self> {
        match event {
            crossterm::event::Event::Key(key) => Some(Self::Key(key)),
            crossterm::event::Event::Mouse(mouse) => Some(Self::Mouse(mouse)),
            crossterm::event::Event::Resize(w, h) => Some(Self::Resize(w, h)),
            crossterm::event::Event::FocusGained => Some(Self::FocusGained),
            crossterm::event::Event::FocusLost => Some(Self::FocusLost),
            crossterm::event::Event::Paste(text) => Some(Self::Paste(text)),
        }
    }

    /// Check if this is a key event with the given code.
    pub(crate) fn is_key_code(&self, code: KeyCode) -> bool {
        matches!(self, Self::Key(KeyEvent { code: c, .. }) if *c == code)
    }

    /// Check if this is a plain character key (no modifiers).
    pub(crate) fn is_plain_char(&self, ch: char) -> bool {
        matches!(
            self,
            Self::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            }) if *c == ch && modifiers.is_empty()
        )
    }

    /// Check if this is a Ctrl+character key.
    pub(crate) fn is_ctrl_char(&self, ch: char) -> bool {
        matches!(
            self,
            Self::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            }) if *c == ch && modifiers.contains(KeyModifiers::CONTROL)
        )
    }

    /// Check if this is an Alt+character key.
    pub(crate) fn is_alt_char(&self, ch: char) -> bool {
        matches!(
            self,
            Self::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            }) if *c == ch && modifiers.contains(KeyModifiers::ALT)
        )
    }

    /// Check if this is a Tab key.
    pub(crate) fn is_tab(&self) -> bool {
        self.is_key_code(KeyCode::Tab)
    }

    /// Check if this is a Shift+Tab (BackTab) key.
    pub(crate) fn is_backtab(&self) -> bool {
        self.is_key_code(KeyCode::BackTab)
    }

    /// Check if this is an Enter/Return key.
    pub(crate) fn is_enter(&self) -> bool {
        self.is_key_code(KeyCode::Enter)
    }

    /// Check if this is an Escape key.
    pub(crate) fn is_escape(&self) -> bool {
        self.is_key_code(KeyCode::Esc)
    }

    /// Check if this is an Up arrow key.
    pub(crate) fn is_up(&self) -> bool {
        self.is_key_code(KeyCode::Up)
    }

    /// Check if this is a Down arrow key.
    pub(crate) fn is_down(&self) -> bool {
        self.is_key_code(KeyCode::Down)
    }

    /// Check if this is a Left arrow key.
    pub(crate) fn is_left(&self) -> bool {
        self.is_key_code(KeyCode::Left)
    }

    /// Check if this is a Right arrow key.
    pub(crate) fn is_right(&self) -> bool {
        self.is_key_code(KeyCode::Right)
    }

    /// Check if this is a mouse down event.
    pub(crate) fn is_mouse_down(&self, button: MouseButton) -> bool {
        matches!(
            self,
            Self::Mouse(MouseEvent {
                kind: MouseEventKind::Down(b),
                ..
            }) if *b == button
        )
    }

    /// Check if this is a mouse click (down) on the left button.
    pub(crate) fn is_left_click(&self) -> bool {
        self.is_mouse_down(MouseButton::Left)
    }

    /// Check if this is a scroll up event.
    pub(crate) fn is_scroll_up(&self) -> bool {
        matches!(
            self,
            Self::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                ..
            })
        )
    }

    /// Check if this is a scroll down event.
    pub(crate) fn is_scroll_down(&self) -> bool {
        matches!(
            self,
            Self::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                ..
            })
        )
    }

    /// Get the mouse position if this is a mouse event.
    pub(crate) fn mouse_position(&self) -> Option<(u16, u16)> {
        match self {
            Self::Mouse(MouseEvent { column, row, .. }) => Some((*column, *row)),
            _ => None,
        }
    }

    /// Get the character if this is a character key event.
    pub(crate) fn char(&self) -> Option<char> {
        match self {
            Self::Key(KeyEvent {
                code: KeyCode::Char(c),
                ..
            }) => Some(*c),
            _ => None,
        }
    }

    /// Get the modifiers if this is a key event.
    #[cfg(test)]
    pub(crate) fn modifiers(&self) -> Option<KeyModifiers> {
        match self {
            Self::Key(KeyEvent { modifiers, .. }) => Some(*modifiers),
            _ => None,
        }
    }
}

impl From<KeyEvent> for UiEvent {
    fn from(key: KeyEvent) -> Self {
        Self::Key(key)
    }
}

impl From<MouseEvent> for UiEvent {
    fn from(mouse: MouseEvent) -> Self {
        Self::Mouse(mouse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_is_plain_char() {
        let event = UiEvent::Key(key_event(KeyCode::Char('a'), KeyModifiers::empty()));
        assert!(event.is_plain_char('a'));
        assert!(!event.is_plain_char('b'));

        let ctrl_event = UiEvent::Key(key_event(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert!(!ctrl_event.is_plain_char('a'));
    }

    #[test]
    fn test_is_ctrl_char() {
        let event = UiEvent::Key(key_event(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(event.is_ctrl_char('c'));
        assert!(!event.is_ctrl_char('a'));

        let plain_event = UiEvent::Key(key_event(KeyCode::Char('c'), KeyModifiers::empty()));
        assert!(!plain_event.is_ctrl_char('c'));
    }

    #[test]
    fn test_is_alt_char() {
        let event = UiEvent::Key(key_event(KeyCode::Char('x'), KeyModifiers::ALT));
        assert!(event.is_alt_char('x'));
    }

    #[test]
    fn test_navigation_keys() {
        assert!(UiEvent::Key(key_event(KeyCode::Tab, KeyModifiers::empty())).is_tab());
        assert!(UiEvent::Key(key_event(KeyCode::BackTab, KeyModifiers::SHIFT)).is_backtab());
        assert!(UiEvent::Key(key_event(KeyCode::Enter, KeyModifiers::empty())).is_enter());
        assert!(UiEvent::Key(key_event(KeyCode::Esc, KeyModifiers::empty())).is_escape());
        assert!(UiEvent::Key(key_event(KeyCode::Up, KeyModifiers::empty())).is_up());
        assert!(UiEvent::Key(key_event(KeyCode::Down, KeyModifiers::empty())).is_down());
        assert!(UiEvent::Key(key_event(KeyCode::Left, KeyModifiers::empty())).is_left());
        assert!(UiEvent::Key(key_event(KeyCode::Right, KeyModifiers::empty())).is_right());
    }

    #[test]
    fn test_char_extraction() {
        let event = UiEvent::Key(key_event(KeyCode::Char('z'), KeyModifiers::empty()));
        assert_eq!(event.char(), Some('z'));

        let event = UiEvent::Key(key_event(KeyCode::Enter, KeyModifiers::empty()));
        assert_eq!(event.char(), None);
    }

    #[test]
    fn test_from_crossterm() {
        let key_event =
            crossterm::event::Event::Key(key_event(KeyCode::Char('a'), KeyModifiers::empty()));
        let ui_event = UiEvent::from_crossterm(key_event);
        assert!(ui_event.is_some());
        assert!(ui_event.unwrap().is_plain_char('a'));

        let resize_event = crossterm::event::Event::Resize(80, 24);
        let ui_event = UiEvent::from_crossterm(resize_event);
        assert_eq!(ui_event, Some(UiEvent::Resize(80, 24)));
    }
}
