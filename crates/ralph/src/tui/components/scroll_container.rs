//! Scrollable line container component for the Ralph TUI.
//!
//! Responsibilities:
//! - Render a line-oriented scrollable viewport (logs/details-like) with a lightweight scrollbar.
//! - Maintain vertical scroll offset via `tui_scrollview::ScrollViewState`.
//! - Support sticky-scroll ("follow tail") behavior: when sticky is enabled, new lines keep the view pinned to bottom;
//!   manual scrolling disables sticky until user reaches bottom or explicitly re-enables it.
//! - Handle mouse wheel scrolling and common keyboard scrolling keys when focused.
//!
//! Not handled here:
//! - ANSI terminal emulation rendering (tui-term / vt100); callers must pre-render into lines if needed.
//! - Arbitrary child widget composition/clipping; this component is intentionally line-based.
//! - Global keybinding policy (e.g., whether `j/k` means scroll); parent mode handlers may also intercept.
//!
//! Invariants/assumptions:
//! - Line count is expected to be <= `u16::MAX` for perfect offset fidelity; larger counts are clamped.
//! - Component mutates only when focused and enabled (except `set_lines`/`push_line` APIs).

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};
use tui_scrollview::ScrollViewState;

use crate::tui::{
    App,
    foundation::{Component, ComponentId, FocusId, FocusManager, RenderCtx, UiEvent},
};

use super::util::{clamp_scroll_offset, max_scroll_offset, rect_contains, scrollbar_thumb};

/// Messages produced by `ScrollableContainerComponent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScrollableContainerMessage {
    /// Scroll position changed (new offset, sticky state).
    Scrolled { offset: usize, sticky: bool },
    /// Sticky mode changed (new sticky).
    StickyChanged { sticky: bool },
    /// No externally relevant action.
    Noop,
}

/// Line-oriented scroll container with sticky-follow support.
pub(crate) struct ScrollableContainerComponent {
    id: ComponentId,
    focus_local: u16,
    enabled: bool,

    title: Option<String>,

    lines: Vec<String>,
    scroll_state: ScrollViewState,

    sticky: bool,
    last_viewport_lines: usize,
    last_area: Option<Rect>,
}

impl ScrollableContainerComponent {
    pub(crate) fn new(id: ComponentId, focus_local: u16) -> Self {
        Self {
            id,
            focus_local,
            enabled: true,
            title: None,
            lines: Vec::new(),
            scroll_state: ScrollViewState::default(),
            sticky: true,
            last_viewport_lines: 1,
            last_area: None,
        }
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    pub(crate) fn focus_id(&self) -> FocusId {
        FocusId::new(self.id, self.focus_local)
    }

    pub(crate) fn set_sticky(&mut self, sticky: bool) {
        self.sticky = sticky;
        if sticky {
            self.scroll_to_bottom();
        }
    }

    pub(crate) fn is_sticky(&self) -> bool {
        self.sticky
    }

    pub(crate) fn set_lines(&mut self, lines: Vec<String>) {
        self.lines = lines;
        if self.sticky {
            self.scroll_to_bottom();
        } else {
            self.clamp_offset();
        }
    }

    pub(crate) fn push_line(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
        if self.sticky {
            self.scroll_to_bottom();
        } else {
            self.clamp_offset();
        }
    }

    fn offset(&self) -> usize {
        self.scroll_state.offset().y as usize
    }

    fn set_offset(&mut self, y: usize) {
        let y = y.min(u16::MAX as usize) as u16;
        self.scroll_state
            .set_offset(ratatui::layout::Position::new(0, y));
    }

    fn total_lines(&self) -> usize {
        self.lines.len()
    }

    fn max_offset(&self) -> usize {
        max_scroll_offset(self.total_lines(), self.last_viewport_lines).min(u16::MAX as usize)
    }

    fn clamp_offset(&mut self) {
        let off = clamp_scroll_offset(self.offset(), self.total_lines(), self.last_viewport_lines);
        self.set_offset(off);
    }

    fn scroll_to_bottom(&mut self) {
        let off = self.max_offset();
        self.set_offset(off);
    }

    fn scroll_by(&mut self, delta: isize) {
        if delta == 0 {
            return;
        }
        let cur = self.offset() as isize;
        let mut next = cur.saturating_add(delta);
        if next < 0 {
            next = 0;
        }

        let max_off = self.max_offset() as isize;
        if next > max_off {
            next = max_off;
        }

        let next_u = next as usize;
        self.set_offset(next_u);

        // Sticky logic: any upward movement disables; reaching bottom re-enables.
        self.sticky = next_u >= self.max_offset();
    }

    fn is_focused(&self, focus: &FocusManager) -> bool {
        focus.is_focused(self.focus_id())
    }

    fn draw_scrollbar(
        &self,
        f: &mut Frame<'_>,
        track: Rect,
        offset: usize,
        viewport: usize,
        total: usize,
    ) {
        if track.width == 0 || track.height == 0 {
            return;
        }
        let (thumb_y, thumb_h) = scrollbar_thumb(track.height, offset, viewport, total);
        let x = track.x;
        let track_style = Style::default().fg(Color::DarkGray);
        let thumb_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);

        let buf = f.buffer_mut();
        for dy in 0..track.height {
            let y = track.y.saturating_add(dy);
            let is_thumb = dy >= thumb_y && dy < thumb_y.saturating_add(thumb_h);
            let cell = &mut buf[(x, y)];
            if is_thumb {
                cell.set_symbol("█").set_style(thumb_style);
            } else {
                cell.set_symbol("│").set_style(track_style);
            }
        }
    }

    fn handle_event_impl(
        &mut self,
        event: &UiEvent,
        focus: &mut FocusManager,
    ) -> Option<ScrollableContainerMessage> {
        if !self.enabled {
            return None;
        }

        // Click-to-focus for forgiving UX (full widget area).
        if event.is_left_click()
            && let (Some((x, y)), Some(area)) = (event.mouse_position(), self.last_area)
            && rect_contains(area, x, y)
        {
            focus.focus(self.focus_id());
        }

        if !self.is_focused(focus) {
            return None;
        }

        // Mouse wheel scrolling.
        if event.is_scroll_up() {
            self.scroll_by(-3);
            return Some(ScrollableContainerMessage::Scrolled {
                offset: self.offset(),
                sticky: self.sticky,
            });
        }
        if event.is_scroll_down() {
            self.scroll_by(3);
            return Some(ScrollableContainerMessage::Scrolled {
                offset: self.offset(),
                sticky: self.sticky,
            });
        }

        // Keyboard scrolling.
        if let UiEvent::Key(k) = event {
            match k.code {
                KeyCode::Up => {
                    self.scroll_by(-1);
                    return Some(ScrollableContainerMessage::Scrolled {
                        offset: self.offset(),
                        sticky: self.sticky,
                    });
                }
                KeyCode::Down => {
                    self.scroll_by(1);
                    return Some(ScrollableContainerMessage::Scrolled {
                        offset: self.offset(),
                        sticky: self.sticky,
                    });
                }
                KeyCode::PageUp => {
                    let jump = self.last_viewport_lines.max(1) as isize;
                    self.scroll_by(-jump);
                    return Some(ScrollableContainerMessage::Scrolled {
                        offset: self.offset(),
                        sticky: self.sticky,
                    });
                }
                KeyCode::PageDown => {
                    let jump = self.last_viewport_lines.max(1) as isize;
                    self.scroll_by(jump);
                    return Some(ScrollableContainerMessage::Scrolled {
                        offset: self.offset(),
                        sticky: self.sticky,
                    });
                }
                KeyCode::Home => {
                    self.set_offset(0);
                    self.sticky = false;
                    return Some(ScrollableContainerMessage::Scrolled {
                        offset: 0,
                        sticky: self.sticky,
                    });
                }
                KeyCode::End => {
                    self.scroll_to_bottom();
                    self.sticky = true;
                    return Some(ScrollableContainerMessage::Scrolled {
                        offset: self.offset(),
                        sticky: self.sticky,
                    });
                }
                _ => {}
            }
        }

        Some(ScrollableContainerMessage::Noop)
    }
}

impl Component for ScrollableContainerComponent {
    type Message = ScrollableContainerMessage;

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, _app: &App, ctx: &mut RenderCtx<'_>) {
        self.last_area = Some(area);

        let title = self.title.clone().unwrap_or_else(|| "Scroll".to_string());
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        ctx.register_focus(self.focus_id(), area, self.enabled);

        // Reserve 1 column for scrollbar if possible.
        let (content_area, scrollbar_area) = if inner.width >= 2 {
            (
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width.saturating_sub(1),
                    inner.height,
                ),
                Rect::new(
                    inner.x.saturating_add(inner.width.saturating_sub(1)),
                    inner.y,
                    1,
                    inner.height,
                ),
            )
        } else {
            (inner, Rect::new(inner.x, inner.y, 0, 0))
        };

        self.last_viewport_lines = content_area.height.max(1) as usize;

        // Sticky-follow: if sticky, keep pinned to bottom after any content change.
        if self.sticky {
            self.scroll_to_bottom();
        } else {
            self.clamp_offset();
        }

        let offset = self.offset();
        let viewport = self.last_viewport_lines;
        let total = self.total_lines();

        // Render visible slice only (cheap even for huge logs).
        let start = offset.min(total);
        let end = start.saturating_add(viewport).min(total);

        let mut text_lines: Vec<Line<'static>> = Vec::with_capacity(end.saturating_sub(start));
        for s in self.lines[start..end].iter() {
            text_lines.push(Line::from(Span::raw(s.clone())));
        }
        if text_lines.is_empty() {
            text_lines.push(Line::from(Span::styled(
                "(empty)",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        let p = Paragraph::new(Text::from(text_lines));
        f.render_widget(p, content_area);

        if scrollbar_area.width == 1 && inner.height > 0 {
            self.draw_scrollbar(f, scrollbar_area, offset, viewport, total);
        }
    }

    fn handle_event(
        &mut self,
        event: &UiEvent,
        _app: &App,
        focus: &mut FocusManager,
    ) -> Option<Self::Message> {
        self.handle_event_impl(event, focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticky_pins_to_bottom_on_set_lines() {
        let mut c = ScrollableContainerComponent::new(ComponentId::new("scroll_container", 0), 0);
        c.last_viewport_lines = 3;

        c.set_lines(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
        ]);

        assert!(c.is_sticky());
        assert_eq!(c.offset(), 2); // 5 total, viewport 3 => max offset 2
    }

    #[test]
    fn manual_scroll_disables_sticky_until_bottom() {
        let mut c = ScrollableContainerComponent::new(ComponentId::new("scroll_container", 0), 0);
        c.last_viewport_lines = 3;
        c.set_lines(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
        ]);
        assert!(c.is_sticky());
        assert_eq!(c.offset(), 2);

        c.scroll_by(-1);
        assert!(!c.is_sticky());
        assert_eq!(c.offset(), 1);

        c.scroll_by(999);
        assert!(c.is_sticky());
        assert_eq!(c.offset(), 2);
    }

    #[test]
    fn clamps_offset_when_not_sticky() {
        let mut c = ScrollableContainerComponent::new(ComponentId::new("scroll_container", 0), 0);
        c.last_viewport_lines = 3;
        c.sticky = false;
        c.set_lines(vec!["a".into(), "b".into()]);
        // total=2, viewport=3 => max=0
        assert_eq!(c.offset(), 0);
    }
}
