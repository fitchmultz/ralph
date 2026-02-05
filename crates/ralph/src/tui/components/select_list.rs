//! Select list / dropdown component for the Ralph TUI.
//!
//! Responsibilities:
//! - Provide a focus-aware select/dropdown list with highlighted selection.
//! - Support scrolling/paging for long lists.
//! - Optional type-to-filter (simple case-insensitive substring match).
//! - Keyboard: Up/Down navigate, Enter select, Esc cancel/close, PageUp/PageDown paging.
//! - Mouse: click-to-focus, click item to select, mouse wheel scroll when open.
//!
//! Not handled here:
//! - Fuzzy matching ranking (only simple filter unless upgraded deliberately).
//! - Popover positioning or overlay scope management (parent decides where/when to render).
//! - Multi-select.
//!
//! Invariants/assumptions:
//! - Items are stored as owned `String`s to keep the component self-contained.
//! - When filter is enabled, selection operates on filtered indices but commits the underlying item.

use crossterm::event::{KeyCode, KeyModifiers};
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

use super::util::{ensure_visible_offset, rect_contains, scrollbar_thumb};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectListItem {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelectListMessage {
    Opened,
    Closed,
    Cancelled,
    HighlightChanged { id: String, index: usize },
    Selected { id: String, index: usize },
    FilterChanged(String),
    Noop,
}

pub(crate) struct SelectListComponent {
    id: ComponentId,
    focus_local: u16,
    enabled: bool,

    title: Option<String>,
    open: bool,

    items: Vec<SelectListItem>,
    highlighted: usize,

    scroll: usize,

    type_to_filter: bool,
    filter: String,
    filtered_indices: Vec<usize>,

    last_area: Option<Rect>,
}

impl SelectListComponent {
    pub(crate) fn new(id: ComponentId, focus_local: u16, items: Vec<SelectListItem>) -> Self {
        let mut c = Self {
            id,
            focus_local,
            enabled: true,
            title: None,
            open: false,
            items,
            highlighted: 0,
            scroll: 0,
            type_to_filter: false,
            filter: String::new(),
            filtered_indices: Vec::new(),
            last_area: None,
        };
        c.rebuild_filter();
        c
    }

    pub(crate) fn focus_id(&self) -> FocusId {
        FocusId::new(self.id, self.focus_local)
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    pub(crate) fn set_type_to_filter(&mut self, enabled: bool) {
        self.type_to_filter = enabled;
        if !enabled {
            self.filter.clear();
            self.rebuild_filter();
        }
    }

    pub(crate) fn set_items(&mut self, items: Vec<SelectListItem>) {
        self.items = items;
        self.highlighted = 0;
        self.scroll = 0;
        self.rebuild_filter();
    }

    pub(crate) fn set_open(&mut self, open: bool) {
        self.open = open;
        if !open {
            self.filter.clear();
            self.rebuild_filter();
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn highlighted_item(&self) -> Option<&SelectListItem> {
        self.filtered_indices
            .get(self.highlighted)
            .and_then(|i| self.items.get(*i))
    }

    fn is_focused(&self, focus: &FocusManager) -> bool {
        focus.is_focused(self.focus_id())
    }

    fn rebuild_filter(&mut self) {
        self.filtered_indices.clear();

        if self.items.is_empty() {
            self.highlighted = 0;
            self.scroll = 0;
            return;
        }

        if self.type_to_filter && !self.filter.is_empty() {
            let needle = self.filter.to_lowercase();
            for (idx, it) in self.items.iter().enumerate() {
                if it.label.to_lowercase().contains(&needle)
                    || it.id.to_lowercase().contains(&needle)
                {
                    self.filtered_indices.push(idx);
                }
            }
        } else {
            self.filtered_indices.extend(0..self.items.len());
        }

        if self.filtered_indices.is_empty() {
            self.highlighted = 0;
            self.scroll = 0;
        } else {
            self.highlighted = self
                .highlighted
                .min(self.filtered_indices.len().saturating_sub(1));
            self.scroll = self.scroll.min(self.highlighted);
        }
    }

    fn move_highlight(&mut self, delta: isize, viewport: usize) -> Option<SelectListMessage> {
        if self.filtered_indices.is_empty() {
            return Some(SelectListMessage::Noop);
        }

        let cur = self.highlighted as isize;
        let max_i = self.filtered_indices.len().saturating_sub(1) as isize;
        let mut next = cur.saturating_add(delta);
        if next < 0 {
            next = 0;
        }
        if next > max_i {
            next = max_i;
        }

        let next_u = next as usize;
        if next_u == self.highlighted {
            return Some(SelectListMessage::Noop);
        }

        self.highlighted = next_u;
        self.scroll = ensure_visible_offset(
            self.highlighted,
            self.scroll,
            viewport,
            self.filtered_indices.len(),
        );

        let (id, idx) = self.current_id_index().unwrap();
        Some(SelectListMessage::HighlightChanged { id, index: idx })
    }

    fn current_id_index(&self) -> Option<(String, usize)> {
        let underlying = *self.filtered_indices.get(self.highlighted)?;
        let it = self.items.get(underlying)?;
        Some((it.id.clone(), underlying))
    }

    fn handle_event_impl(
        &mut self,
        event: &UiEvent,
        focus: &mut FocusManager,
        viewport: usize,
    ) -> Option<SelectListMessage> {
        if !self.enabled {
            return None;
        }

        // Click-to-focus + click behavior.
        if event.is_left_click()
            && let (Some((x, y)), Some(area)) = (event.mouse_position(), self.last_area)
            && rect_contains(area, x, y)
        {
            focus.focus(self.focus_id());

            // If open, try selecting by click on item row.
            if self.open && viewport > 0 {
                let block = Block::default().borders(Borders::ALL);
                let inner = block.inner(area);

                // Header lines: 1 for selected line; +1 for filter if enabled.
                let header = 1 + if self.type_to_filter { 1 } else { 0 };
                let list_y0 = inner.y.saturating_add(header as u16);
                if y >= list_y0 && y < inner.y.saturating_add(inner.height) {
                    let row = (y - list_y0) as usize;
                    let filtered_idx = self.scroll.saturating_add(row);
                    if let Some(underlying) = self.filtered_indices.get(filtered_idx).copied() {
                        // Clone id before any mutable borrows
                        let id = self.items.get(underlying).map(|it| it.id.clone());
                        if let Some(id) = id {
                            self.highlighted = filtered_idx;
                            self.open = false;
                            self.filter.clear();
                            self.rebuild_filter();
                            return Some(SelectListMessage::Selected {
                                id,
                                index: underlying,
                            });
                        }
                    }
                }
            }

            // If closed, clicking toggles open.
            if !self.open {
                self.open = true;
                return Some(SelectListMessage::Opened);
            }
        }

        if !self.is_focused(focus) {
            return None;
        }

        // Mouse wheel scroll when open.
        if self.open {
            if event.is_scroll_up() {
                self.scroll = self.scroll.saturating_sub(3);
                return Some(SelectListMessage::Noop);
            }
            if event.is_scroll_down() {
                let max_scroll = self.filtered_indices.len().saturating_sub(viewport.max(1));
                self.scroll = (self.scroll + 3).min(max_scroll);
                return Some(SelectListMessage::Noop);
            }
        }

        // Closed state: Enter/Space opens; Esc cancels.
        if !self.open {
            if event.is_enter() || event.is_plain_char(' ') {
                self.open = true;
                return Some(SelectListMessage::Opened);
            }
            if event.is_escape() {
                return Some(SelectListMessage::Cancelled);
            }
            return Some(SelectListMessage::Noop);
        }

        // Open state controls.
        if event.is_escape() {
            if self.type_to_filter && !self.filter.is_empty() {
                self.filter.clear();
                self.rebuild_filter();
                return Some(SelectListMessage::FilterChanged(self.filter.clone()));
            }
            self.open = false;
            self.filter.clear();
            self.rebuild_filter();
            return Some(SelectListMessage::Closed);
        }

        if event.is_up() {
            return self.move_highlight(-1, viewport);
        }
        if event.is_down() {
            return self.move_highlight(1, viewport);
        }

        if let UiEvent::Key(k) = event {
            match k.code {
                KeyCode::PageUp => return self.move_highlight(-(viewport as isize), viewport),
                KeyCode::PageDown => return self.move_highlight(viewport as isize, viewport),
                KeyCode::Home => {
                    self.highlighted = 0;
                    self.scroll = 0;
                    if let Some((id, idx)) = self.current_id_index() {
                        return Some(SelectListMessage::HighlightChanged { id, index: idx });
                    }
                    return Some(SelectListMessage::Noop);
                }
                KeyCode::End => {
                    if !self.filtered_indices.is_empty() {
                        self.highlighted = self.filtered_indices.len().saturating_sub(1);
                        self.scroll = ensure_visible_offset(
                            self.highlighted,
                            self.scroll,
                            viewport,
                            self.filtered_indices.len(),
                        );
                        if let Some((id, idx)) = self.current_id_index() {
                            return Some(SelectListMessage::HighlightChanged { id, index: idx });
                        }
                    }
                    return Some(SelectListMessage::Noop);
                }
                KeyCode::Enter => {
                    if let Some((id, idx)) = self.current_id_index() {
                        self.open = false;
                        self.filter.clear();
                        self.rebuild_filter();
                        return Some(SelectListMessage::Selected { id, index: idx });
                    }
                    return Some(SelectListMessage::Noop);
                }
                KeyCode::Backspace => {
                    if self.type_to_filter && !self.filter.is_empty() {
                        self.filter.pop();
                        self.rebuild_filter();
                        return Some(SelectListMessage::FilterChanged(self.filter.clone()));
                    }
                    return Some(SelectListMessage::Noop);
                }
                KeyCode::Char(c) => {
                    if self.type_to_filter && k.modifiers.is_empty() {
                        self.filter.push(c);
                        self.rebuild_filter();
                        self.highlighted = 0;
                        self.scroll = 0;
                        return Some(SelectListMessage::FilterChanged(self.filter.clone()));
                    }
                    if self.type_to_filter
                        && k.modifiers.contains(KeyModifiers::CONTROL)
                        && (c == 'u' || c == 'U')
                    {
                        self.filter.clear();
                        self.rebuild_filter();
                        return Some(SelectListMessage::FilterChanged(self.filter.clone()));
                    }
                }
                _ => {}
            }
        }

        Some(SelectListMessage::Noop)
    }

    fn draw_scrollbar(&self, f: &mut Frame<'_>, track: Rect, viewport: usize) {
        if track.width == 0 || track.height == 0 {
            return;
        }
        let total = self.filtered_indices.len();
        let (thumb_y, thumb_h) = scrollbar_thumb(track.height, self.scroll, viewport, total);
        let x = track.x;
        let buf = f.buffer_mut();

        for dy in 0..track.height {
            let y = track.y.saturating_add(dy);
            let is_thumb = dy >= thumb_y && dy < thumb_y.saturating_add(thumb_h);
            let cell = &mut buf[(x, y)];
            if is_thumb {
                cell.set_symbol("█")
                    .set_style(Style::default().fg(Color::White));
            } else {
                cell.set_symbol("│")
                    .set_style(Style::default().fg(Color::DarkGray));
            }
        }
    }
}

impl Component for SelectListComponent {
    type Message = SelectListMessage;

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, _app: &App, ctx: &mut RenderCtx<'_>) {
        self.last_area = Some(area);

        let title = self.title.clone().unwrap_or_else(|| "Select".to_string());
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        ctx.register_focus(self.focus_id(), area, self.enabled);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Header lines: selected summary + optional filter.
        let header_h = 1 + if self.type_to_filter { 1 } else { 0 };
        let header_h_u16 = header_h.min(inner.height as usize) as u16;

        // List area is remaining.
        let list_area = Rect::new(
            inner.x,
            inner.y.saturating_add(header_h_u16),
            inner.width,
            inner.height.saturating_sub(header_h_u16),
        );

        // Selected line (always visible).
        let selected_text = self
            .highlighted_item()
            .map(|it| format!("{}  {}", it.label, if self.open { "▴" } else { "▾" }))
            .unwrap_or_else(|| "(no items)".to_string());

        f.render_widget(
            Paragraph::new(Line::from(Span::raw(selected_text))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        // Filter line (optional).
        if self.type_to_filter && inner.height >= 2 {
            let filter_line = format!("Filter: {}", self.filter);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    filter_line,
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(inner.x, inner.y.saturating_add(1), inner.width, 1),
            );
        }

        if !self.open {
            return;
        }

        // Reserve scrollbar column if possible.
        let (content, scrollbar) = if list_area.width >= 2 {
            (
                Rect::new(
                    list_area.x,
                    list_area.y,
                    list_area.width.saturating_sub(1),
                    list_area.height,
                ),
                Rect::new(
                    list_area
                        .x
                        .saturating_add(list_area.width.saturating_sub(1)),
                    list_area.y,
                    1,
                    list_area.height,
                ),
            )
        } else {
            (list_area, Rect::new(list_area.x, list_area.y, 0, 0))
        };

        let viewport = content.height.max(1) as usize;

        // Ensure highlight visible.
        if !self.filtered_indices.is_empty() {
            self.highlighted = self
                .highlighted
                .min(self.filtered_indices.len().saturating_sub(1));
            self.scroll = ensure_visible_offset(
                self.highlighted,
                self.scroll,
                viewport,
                self.filtered_indices.len(),
            );
        } else {
            self.scroll = 0;
            self.highlighted = 0;
        }

        // Render visible items.
        let mut y = content.y;
        let start = self.scroll.min(self.filtered_indices.len());
        let end = start
            .saturating_add(viewport)
            .min(self.filtered_indices.len());

        if self.filtered_indices.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "(no matches)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ))),
                content,
            );
        } else {
            for (row, filtered_idx) in (start..end).enumerate() {
                let underlying = self.filtered_indices[filtered_idx];
                let it = &self.items[underlying];

                let selected = filtered_idx == self.highlighted;
                let style = if selected {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let line = Line::from(Span::styled(it.label.clone(), style));
                f.render_widget(
                    Paragraph::new(line),
                    Rect::new(content.x, y, content.width, 1),
                );
                y = y.saturating_add(1);
                if row + 1 >= viewport {
                    break;
                }
            }
        }

        if scrollbar.width == 1 && scrollbar.height > 0 {
            self.draw_scrollbar(f, scrollbar, viewport);
        }
    }

    fn handle_event(
        &mut self,
        event: &UiEvent,
        _app: &App,
        focus: &mut FocusManager,
    ) -> Option<Self::Message> {
        // viewport is last known; compute safely using last_area and borders.
        let viewport = self
            .last_area
            .map(|a| {
                let inner = Block::default().borders(Borders::ALL).inner(a);
                let header = 1 + if self.type_to_filter { 1 } else { 0 };
                inner.height.saturating_sub(header as u16).max(1) as usize
            })
            .unwrap_or(1);

        self.handle_event_impl(event, focus, viewport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<SelectListItem> {
        vec![
            SelectListItem {
                id: "a".into(),
                label: "Alpha".into(),
            },
            SelectListItem {
                id: "b".into(),
                label: "Beta".into(),
            },
            SelectListItem {
                id: "g".into(),
                label: "Gamma".into(),
            },
        ]
    }

    #[test]
    fn filter_rebuilds_indices_and_handles_empty() {
        let mut c = SelectListComponent::new(ComponentId::new("select", 0), 0, items());
        c.set_type_to_filter(true);

        c.filter = "zz".into();
        c.rebuild_filter();
        assert!(c.filtered_indices.is_empty());

        c.filter = "a".into();
        c.rebuild_filter();
        assert!(!c.filtered_indices.is_empty());
    }

    #[test]
    fn move_highlight_is_clamped() {
        let mut c = SelectListComponent::new(ComponentId::new("select", 0), 0, items());
        c.set_open(true);
        c.set_type_to_filter(false);

        let _ = c.move_highlight(999, 2);
        assert_eq!(c.highlighted, 2);

        let _ = c.move_highlight(-999, 2);
        assert_eq!(c.highlighted, 0);
    }

    #[test]
    fn selecting_returns_underlying_id_and_index() {
        let mut c = SelectListComponent::new(ComponentId::new("select", 0), 0, items());
        c.set_open(true);
        c.highlighted = 1;
        let sel = c.current_id_index().unwrap();
        assert_eq!(sel.0, "b");
        assert_eq!(sel.1, 1);
    }
}
