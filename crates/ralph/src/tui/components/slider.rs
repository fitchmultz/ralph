//! Slider component for the Ralph TUI.
//!
//! Responsibilities:
//! - Render a horizontal slider with a numeric value label and a filled-bar visualization.
//! - Support keyboard adjustments when focused (Left/Right by step; PageUp/PageDown by page step; Home/End to min/max).
//! - Provide a simple message when the value changes.
//!
//! Not handled here:
//! - Mouse drag interactions (optional enhancement; keep this keyboard-first).
//! - Formatting beyond a simple integer display (callers can set label and ranges).
//!
//! Invariants/assumptions:
//! - `min <= max`; if constructed otherwise, values are normalized by swapping.
//! - Value is always clamped to `[min, max]`.
//! - Component mutates only when focused and enabled.

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::{
    App,
    foundation::{Component, ComponentId, FocusId, FocusManager, RenderCtx, UiEvent},
};

use super::util::rect_contains;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SliderMessage {
    Changed(i64),
    Noop,
}

pub(crate) struct SliderComponent {
    id: ComponentId,
    focus_local: u16,
    enabled: bool,

    label: Option<String>,
    min: i64,
    max: i64,
    value: i64,

    step: i64,
    page_step: i64,

    last_area: Option<Rect>,
}

impl SliderComponent {
    pub(crate) fn new(id: ComponentId, focus_local: u16) -> Self {
        Self {
            id,
            focus_local,
            enabled: true,
            label: None,
            min: 0,
            max: 100,
            value: 0,
            step: 1,
            page_step: 10,
            last_area: None,
        }
    }

    pub(crate) fn focus_id(&self) -> FocusId {
        FocusId::new(self.id, self.focus_local)
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn set_label(&mut self, label: impl Into<String>) {
        self.label = Some(label.into());
    }

    pub(crate) fn set_range(&mut self, min: i64, max: i64) {
        if min <= max {
            self.min = min;
            self.max = max;
        } else {
            self.min = max;
            self.max = min;
        }
        self.value = self.value.clamp(self.min, self.max);
    }

    pub(crate) fn set_steps(&mut self, step: i64, page_step: i64) {
        self.step = step.max(1);
        self.page_step = page_step.max(self.step);
    }

    pub(crate) fn set_value(&mut self, value: i64) {
        self.value = value.clamp(self.min, self.max);
    }

    pub(crate) fn value(&self) -> i64 {
        self.value
    }

    fn is_focused(&self, focus: &FocusManager) -> bool {
        focus.is_focused(self.focus_id())
    }

    fn apply_delta(&mut self, delta: i64) -> Option<SliderMessage> {
        let before = self.value;
        self.value = (self.value.saturating_add(delta)).clamp(self.min, self.max);
        if self.value != before {
            Some(SliderMessage::Changed(self.value))
        } else {
            Some(SliderMessage::Noop)
        }
    }

    fn handle_event_impl(
        &mut self,
        event: &UiEvent,
        focus: &mut FocusManager,
    ) -> Option<SliderMessage> {
        if !self.enabled {
            return None;
        }

        if event.is_left_click()
            && let (Some((x, y)), Some(area)) = (event.mouse_position(), self.last_area)
            && rect_contains(area, x, y)
        {
            focus.focus(self.focus_id());
        }

        if !self.is_focused(focus) {
            return None;
        }

        if let UiEvent::Key(k) = event {
            match k.code {
                KeyCode::Left => return self.apply_delta(-self.step),
                KeyCode::Right => return self.apply_delta(self.step),
                KeyCode::PageUp => return self.apply_delta(self.page_step),
                KeyCode::PageDown => return self.apply_delta(-self.page_step),
                KeyCode::Home => {
                    let before = self.value;
                    self.value = self.min;
                    return Some(if self.value != before {
                        SliderMessage::Changed(self.value)
                    } else {
                        SliderMessage::Noop
                    });
                }
                KeyCode::End => {
                    let before = self.value;
                    self.value = self.max;
                    return Some(if self.value != before {
                        SliderMessage::Changed(self.value)
                    } else {
                        SliderMessage::Noop
                    });
                }
                _ => {}
            }
        }

        Some(SliderMessage::Noop)
    }
}

impl Component for SliderComponent {
    type Message = SliderMessage;

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, _app: &App, ctx: &mut RenderCtx<'_>) {
        self.last_area = Some(area);

        let title = self.label.clone().unwrap_or_else(|| "Slider".to_string());
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        ctx.register_focus(self.focus_id(), area, self.enabled);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Layout: one-line bar + value label (same line).
        let bar_width = inner.width.saturating_sub(8).max(5); // keep minimal room
        let range = (self.max - self.min).max(1);
        let pos = (self.value - self.min).clamp(0, range);

        let filled = ((bar_width as i128) * (pos as i128) / (range as i128))
            .clamp(0, bar_width as i128) as u16;

        let mut bar = String::new();
        bar.push('[');
        for i in 0..bar_width {
            if i < filled {
                bar.push('█');
            } else {
                bar.push('░');
            }
        }
        bar.push(']');

        let value_str = format!("{:>4}", self.value);

        let line = Line::from(vec![
            Span::styled(bar, Style::default().fg(Color::White)),
            Span::raw(" "),
            Span::styled(
                value_str,
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        f.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
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
    fn range_normalizes_and_clamps_value() {
        let mut s = SliderComponent::new(ComponentId::new("slider", 0), 0);
        s.set_range(100, 0);
        assert_eq!(s.min, 0);
        assert_eq!(s.max, 100);

        s.set_value(999);
        assert_eq!(s.value(), 100);
    }

    #[test]
    fn delta_applies_step_and_clamps() {
        let mut s = SliderComponent::new(ComponentId::new("slider", 0), 0);
        s.set_range(0, 10);
        s.set_steps(2, 5);
        s.set_value(9);

        let msg = s.apply_delta(2).unwrap();
        assert_eq!(msg, SliderMessage::Changed(10));

        let msg2 = s.apply_delta(2).unwrap();
        assert_eq!(msg2, SliderMessage::Noop);
    }
}
