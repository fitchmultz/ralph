//! Shared helper utilities for Ralph TUI components.
//!
//! Responsibilities:
//! - Provide small, pure helpers used by multiple components (hit-testing, scrolling clamps).
//! - Provide scrollbar thumb geometry calculation for lightweight in-widget scrollbars.
//!
//! Not handled here:
//! - Rendering full widgets or maintaining component state.
//! - Any app-specific business rules (selection meaning, log policies).
//!
//! Invariants/assumptions:
//! - All computations use saturating/clamping math (never panics on tiny terminals).
//! - Scroll offsets are `usize` but may be clamped to `u16::MAX` when stored in ScrollViewState.

use ratatui::layout::Rect;

pub(crate) fn rect_contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

pub(crate) fn max_scroll_offset(total_items: usize, viewport_items: usize) -> usize {
    let viewport_items = viewport_items.max(1);
    total_items.saturating_sub(viewport_items)
}

pub(crate) fn clamp_scroll_offset(
    offset: usize,
    total_items: usize,
    viewport_items: usize,
) -> usize {
    offset.min(max_scroll_offset(total_items, viewport_items))
}

/// Ensures `selected` is visible inside `[offset, offset + viewport)`, returning a new offset.
pub(crate) fn ensure_visible_offset(
    selected: usize,
    offset: usize,
    viewport_items: usize,
    total_items: usize,
) -> usize {
    if total_items == 0 {
        return 0;
    }
    let viewport_items = viewport_items.max(1);
    let selected = selected.min(total_items.saturating_sub(1));
    let mut offset = clamp_scroll_offset(offset, total_items, viewport_items);

    if selected < offset {
        offset = selected;
    } else {
        let last_visible = offset.saturating_add(viewport_items).saturating_sub(1);
        if selected > last_visible {
            offset = selected.saturating_sub(viewport_items.saturating_sub(1));
        }
    }

    clamp_scroll_offset(offset, total_items, viewport_items)
}

/// Compute a vertical scrollbar thumb for a track with `track_height` cells.
///
/// Returns `(thumb_start_y, thumb_height)` where both are in `0..=track_height`.
pub(crate) fn scrollbar_thumb(
    track_height: u16,
    offset: usize,
    viewport: usize,
    total: usize,
) -> (u16, u16) {
    if track_height == 0 || total == 0 {
        return (0, 0);
    }

    let viewport = viewport.max(1);
    if total <= viewport {
        return (0, track_height);
    }

    let max_off = max_scroll_offset(total, viewport).max(1);

    // Thumb size proportional to viewport/total, min 1.
    let mut thumb_h = ((track_height as u32) * (viewport as u32) / (total as u32)).max(1) as u16;
    thumb_h = thumb_h.min(track_height);

    // Thumb position proportional to offset/max_off.
    let travel = track_height.saturating_sub(thumb_h);
    let off = offset.min(max_off);
    let thumb_y = ((travel as u32) * (off as u32) / (max_off as u32)) as u16;

    (thumb_y.min(travel), thumb_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_handles_edges() {
        let r = Rect::new(5, 5, 10, 10);
        assert!(rect_contains(r, 5, 5));
        assert!(rect_contains(r, 14, 14));
        assert!(!rect_contains(r, 15, 5));
        assert!(!rect_contains(r, 5, 15));
    }

    #[test]
    fn clamp_and_max_scroll_are_saturating() {
        assert_eq!(max_scroll_offset(0, 10), 0);
        assert_eq!(max_scroll_offset(5, 10), 0);
        assert_eq!(max_scroll_offset(20, 10), 10);

        assert_eq!(clamp_scroll_offset(99, 5, 10), 0);
        assert_eq!(clamp_scroll_offset(7, 20, 10), 7);
        assert_eq!(clamp_scroll_offset(99, 20, 10), 10);
    }

    #[test]
    fn ensure_visible_moves_offset_minimally() {
        // total=100, viewport=10
        assert_eq!(ensure_visible_offset(0, 0, 10, 100), 0);
        assert_eq!(ensure_visible_offset(9, 0, 10, 100), 0);
        assert_eq!(ensure_visible_offset(10, 0, 10, 100), 1);
        assert_eq!(ensure_visible_offset(50, 0, 10, 100), 41);
    }

    #[test]
    fn scrollbar_thumb_handles_small_and_full_cases() {
        assert_eq!(scrollbar_thumb(0, 0, 10, 100), (0, 0));
        assert_eq!(scrollbar_thumb(10, 0, 100, 50), (0, 10)); // total <= viewport => full thumb
    }

    #[test]
    fn scrollbar_thumb_is_within_track() {
        let (y, h) = scrollbar_thumb(10, 50, 10, 100);
        assert!((1..=10).contains(&h));
        assert!(y <= 10 - h);
    }
}
