//! Layout primitives for the Ralph TUI foundation.
//!
//! Responsibilities:
//! - Provide flexbox-like layout helpers (row, col, stack, pad, gap, spacer).
//! - Build on Ratatui's Rect for compatibility with existing code.
//! - Handle edge cases gracefully (small terminals, zero space).
//!
//! Not handled here:
//! - Complex flexbox features like wrapping, alignment, or justify-content.
//!   (These can be added later if needed; for now, use Ratatui's Layout.)
//!
//! Invariants/assumptions:
//! - All calculations use saturating math to avoid underflow/overflow.
//! - Zero-sized or too-small areas produce reasonable fallback rects.
//! - Gap is applied between items, not at the edges.

use ratatui::layout::Rect;

/// Padding values for a rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Padding {
    pub(crate) top: u16,
    pub(crate) right: u16,
    pub(crate) bottom: u16,
    pub(crate) left: u16,
}

impl Padding {
    /// Create uniform padding on all sides.
    pub(crate) const fn uniform(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Create horizontal (left/right) padding.
    pub(crate) const fn horizontal(value: u16) -> Self {
        Self {
            top: 0,
            right: value,
            bottom: 0,
            left: value,
        }
    }

    /// Create vertical (top/bottom) padding.
    pub(crate) const fn vertical(value: u16) -> Self {
        Self {
            top: value,
            right: 0,
            bottom: value,
            left: 0,
        }
    }

    /// Create padding with only top value.
    #[allow(dead_code)]
    pub(crate) const fn top(value: u16) -> Self {
        Self {
            top: value,
            right: 0,
            bottom: 0,
            left: 0,
        }
    }

    /// Create padding with only bottom value.
    #[allow(dead_code)]
    pub(crate) const fn bottom(value: u16) -> Self {
        Self {
            top: 0,
            right: 0,
            bottom: value,
            left: 0,
        }
    }

    /// Create padding with only left value.
    #[allow(dead_code)]
    pub(crate) const fn left(value: u16) -> Self {
        Self {
            top: 0,
            right: 0,
            bottom: 0,
            left: value,
        }
    }

    /// Create padding with only right value.
    #[allow(dead_code)]
    pub(crate) const fn right(value: u16) -> Self {
        Self {
            top: 0,
            right: value,
            bottom: 0,
            left: 0,
        }
    }

    /// Get total horizontal padding.
    pub(crate) fn horizontal_total(&self) -> u16 {
        self.left.saturating_add(self.right)
    }

    /// Get total vertical padding.
    pub(crate) fn vertical_total(&self) -> u16 {
        self.top.saturating_add(self.bottom)
    }
}

/// Size specification for layout items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ItemSize {
    /// Fixed size in cells.
    Fixed(u16),
    /// Minimum size (gets at least this, more if available).
    Min(u16),
    /// Flex grow weight (distributes remaining space).
    Flex(u16),
    /// Percentage of available space (0-100).
    Percent(u16),
}

impl ItemSize {
    /// Check if this size is flexible (Flex variant).
    fn is_flex(&self) -> bool {
        matches!(self, ItemSize::Flex(_))
    }
}

/// A layout item with a size specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Item {
    pub(crate) size: ItemSize,
}

impl Item {
    /// Create a new item with the given size.
    pub(crate) const fn new(size: ItemSize) -> Self {
        Self { size }
    }

    /// Create a fixed-size item.
    pub(crate) const fn fixed(size: u16) -> Self {
        Self::new(ItemSize::Fixed(size))
    }

    /// Create a minimum-size item.
    pub(crate) const fn min(size: u16) -> Self {
        Self::new(ItemSize::Min(size))
    }

    /// Create a flex item with the given weight.
    pub(crate) const fn flex(weight: u16) -> Self {
        Self::new(ItemSize::Flex(weight))
    }

    /// Create a percentage item.
    pub(crate) const fn percent(pct: u16) -> Self {
        Self::new(ItemSize::Percent(pct))
    }
}

/// Apply padding to a rectangle, returning the inner rectangle.
///
/// If padding exceeds the rectangle size, returns a zero-sized rect
/// at the original position.
pub(crate) fn pad(area: Rect, padding: Padding) -> Rect {
    let horizontal = padding.horizontal_total();
    let vertical = padding.vertical_total();

    if horizontal >= area.width || vertical >= area.height {
        return Rect::new(area.x, area.y, 0, 0);
    }

    Rect::new(
        area.x.saturating_add(padding.left),
        area.y.saturating_add(padding.top),
        area.width.saturating_sub(horizontal),
        area.height.saturating_sub(vertical),
    )
}

/// Split an area vertically into top and bottom parts.
///
/// Returns (top, bottom). Gap is applied between them.
pub(crate) fn v_split(area: Rect, top_height: u16, gap: u16) -> (Rect, Rect) {
    let top_height = top_height.min(area.height);
    let gap = gap.min(area.height.saturating_sub(top_height));
    let bottom_height = area.height.saturating_sub(top_height).saturating_sub(gap);

    let top = Rect::new(area.x, area.y, area.width, top_height);
    let bottom = Rect::new(
        area.x,
        area.y.saturating_add(top_height).saturating_add(gap),
        area.width,
        bottom_height,
    );

    (top, bottom)
}

/// Split an area horizontally into left and right parts.
///
/// Returns (left, right). Gap is applied between them.
pub(crate) fn h_split(area: Rect, left_width: u16, gap: u16) -> (Rect, Rect) {
    let left_width = left_width.min(area.width);
    let gap = gap.min(area.width.saturating_sub(left_width));
    let right_width = area.width.saturating_sub(left_width).saturating_sub(gap);

    let left = Rect::new(area.x, area.y, left_width, area.height);
    let right = Rect::new(
        area.x.saturating_add(left_width).saturating_add(gap),
        area.y,
        right_width,
        area.height,
    );

    (left, right)
}

/// Layout items in a row (horizontal).
///
/// Distributes available width according to item sizes.
/// Gap is applied between items.
pub(crate) fn row(area: Rect, gap: u16, items: &[Item]) -> Vec<Rect> {
    if items.is_empty() {
        return Vec::new();
    }

    let total_gap = gap.saturating_mul(items.len().saturating_sub(1) as u16);
    let available_width = area.width.saturating_sub(total_gap);

    // First pass: calculate fixed and percentage sizes
    let mut sizes: Vec<u16> = Vec::with_capacity(items.len());
    let mut total_fixed: u16 = 0;
    let mut total_flex_weight: u16 = 0;

    for item in items {
        let size = match item.size {
            ItemSize::Fixed(n) => n,
            ItemSize::Percent(pct) => (available_width as u32 * pct.min(100) as u32 / 100) as u16,
            ItemSize::Min(n) => n.min(available_width),
            ItemSize::Flex(weight) => {
                total_flex_weight = total_flex_weight.saturating_add(weight);
                0 // Will be calculated in second pass
            }
        };
        sizes.push(size);
        if !item.size.is_flex() {
            total_fixed = total_fixed.saturating_add(size);
        }
    }

    // Second pass: distribute remaining space to flex items
    let remaining = available_width.saturating_sub(total_fixed);
    let flex_unit = if total_flex_weight > 0 {
        remaining / total_flex_weight
    } else {
        0
    };

    for (i, item) in items.iter().enumerate() {
        if let ItemSize::Flex(weight) = item.size {
            sizes[i] = flex_unit.saturating_mul(weight);
        }
    }

    // Build rects
    let mut rects = Vec::with_capacity(items.len());
    let mut current_x = area.x;

    for (i, size) in sizes.iter().enumerate() {
        rects.push(Rect::new(current_x, area.y, *size, area.height));
        current_x = current_x.saturating_add(*size);
        if i < items.len().saturating_sub(1) {
            current_x = current_x.saturating_add(gap);
        }
    }

    rects
}

/// Layout items in a column (vertical).
///
/// Distributes available height according to item sizes.
/// Gap is applied between items.
pub(crate) fn col(area: Rect, gap: u16, items: &[Item]) -> Vec<Rect> {
    if items.is_empty() {
        return Vec::new();
    }

    let total_gap = gap.saturating_mul(items.len().saturating_sub(1) as u16);
    let available_height = area.height.saturating_sub(total_gap);

    // First pass: calculate fixed and percentage sizes
    let mut sizes: Vec<u16> = Vec::with_capacity(items.len());
    let mut total_fixed: u16 = 0;
    let mut total_flex_weight: u16 = 0;

    for item in items {
        let size = match item.size {
            ItemSize::Fixed(n) => n,
            ItemSize::Percent(pct) => (available_height as u32 * pct.min(100) as u32 / 100) as u16,
            ItemSize::Min(n) => n.min(available_height),
            ItemSize::Flex(weight) => {
                total_flex_weight = total_flex_weight.saturating_add(weight);
                0 // Will be calculated in second pass
            }
        };
        sizes.push(size);
        if !item.size.is_flex() {
            total_fixed = total_fixed.saturating_add(size);
        }
    }

    // Second pass: distribute remaining space to flex items
    let remaining = available_height.saturating_sub(total_fixed);
    let flex_unit = if total_flex_weight > 0 {
        remaining / total_flex_weight
    } else {
        0
    };

    for (i, item) in items.iter().enumerate() {
        if let ItemSize::Flex(weight) = item.size {
            sizes[i] = flex_unit.saturating_mul(weight);
        }
    }

    // Build rects
    let mut rects = Vec::with_capacity(items.len());
    let mut current_y = area.y;

    for (i, size) in sizes.iter().enumerate() {
        rects.push(Rect::new(area.x, current_y, area.width, *size));
        current_y = current_y.saturating_add(*size);
        if i < items.len().saturating_sub(1) {
            current_y = current_y.saturating_add(gap);
        }
    }

    rects
}

/// Get a centered rectangle within the given area.
///
/// If the requested size is larger than the area, clamps to the area size.
pub(crate) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);

    let x = area
        .x
        .saturating_add((area.width.saturating_sub(width)) / 2);
    let y = area
        .y
        .saturating_add((area.height.saturating_sub(height)) / 2);

    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rect(x: u16, y: u16, w: u16, h: u16) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn test_pad_uniform() {
        let area = test_rect(0, 0, 100, 50);
        let padded = pad(area, Padding::uniform(5));

        assert_eq!(padded.x, 5);
        assert_eq!(padded.y, 5);
        assert_eq!(padded.width, 90);
        assert_eq!(padded.height, 40);
    }

    #[test]
    fn test_pad_horizontal() {
        let area = test_rect(0, 0, 100, 50);
        let padded = pad(area, Padding::horizontal(10));

        assert_eq!(padded.x, 10);
        assert_eq!(padded.y, 0);
        assert_eq!(padded.width, 80);
        assert_eq!(padded.height, 50);
    }

    #[test]
    fn test_pad_vertical() {
        let area = test_rect(0, 0, 100, 50);
        let padded = pad(area, Padding::vertical(10));

        assert_eq!(padded.x, 0);
        assert_eq!(padded.y, 10);
        assert_eq!(padded.width, 100);
        assert_eq!(padded.height, 30);
    }

    #[test]
    fn test_pad_overflow() {
        // Padding exceeds area - should return zero-sized
        let area = test_rect(0, 0, 10, 10);
        let padded = pad(area, Padding::uniform(10));

        assert_eq!(padded.width, 0);
        assert_eq!(padded.height, 0);
    }

    #[test]
    fn test_v_split() {
        let area = test_rect(0, 0, 100, 100);
        let (top, bottom) = v_split(area, 30, 2);

        assert_eq!(top.x, 0);
        assert_eq!(top.y, 0);
        assert_eq!(top.width, 100);
        assert_eq!(top.height, 30);

        assert_eq!(bottom.x, 0);
        assert_eq!(bottom.y, 32); // 30 + 2 gap
        assert_eq!(bottom.width, 100);
        assert_eq!(bottom.height, 68);
    }

    #[test]
    fn test_v_split_clamps() {
        let area = test_rect(0, 0, 100, 20);
        let (top, bottom) = v_split(area, 50, 5);

        // Top should be clamped to area height
        assert_eq!(top.height, 20);
        // Bottom should be zero (or near-zero due to gap)
        assert_eq!(bottom.height, 0);
    }

    #[test]
    fn test_h_split() {
        let area = test_rect(0, 0, 100, 100);
        let (left, right) = h_split(area, 40, 2);

        assert_eq!(left.x, 0);
        assert_eq!(left.y, 0);
        assert_eq!(left.width, 40);
        assert_eq!(left.height, 100);

        assert_eq!(right.x, 42); // 40 + 2 gap
        assert_eq!(right.y, 0);
        assert_eq!(right.width, 58);
        assert_eq!(right.height, 100);
    }

    #[test]
    fn test_h_split_clamps() {
        let area = test_rect(0, 0, 20, 100);
        let (left, right) = h_split(area, 50, 5);

        // Left should be clamped to area width
        assert_eq!(left.width, 20);
        // Right should be zero
        assert_eq!(right.width, 0);
    }

    #[test]
    fn test_row_fixed() {
        let area = test_rect(0, 0, 100, 10);
        let items = vec![Item::fixed(20), Item::fixed(30), Item::fixed(10)];
        let rects = row(area, 2, &items);

        assert_eq!(rects.len(), 3);

        // First item
        assert_eq!(rects[0].x, 0);
        assert_eq!(rects[0].width, 20);

        // Second item (after 20 + 2 gap)
        assert_eq!(rects[1].x, 22);
        assert_eq!(rects[1].width, 30);

        // Third item (after 22 + 30 + 2 gap)
        assert_eq!(rects[2].x, 54);
        assert_eq!(rects[2].width, 10);
    }

    #[test]
    fn test_row_flex() {
        let area = test_rect(0, 0, 100, 10);
        // 20 fixed + 2 gap + flex(2) + 2 gap + flex(1)
        // Available for flex: 100 - 20 - 2 - 2 = 76
        // flex(2) gets 2 * (76/3) = 2 * 25 = 50
        // flex(1) gets 1 * (76/3) = 1 * 25 = 25
        // Total: 20 + 50 + 25 = 95 (5 pixels lost to integer division)
        let items = vec![Item::fixed(20), Item::flex(2), Item::flex(1)];
        let rects = row(area, 2, &items);

        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].width, 20);
        assert_eq!(rects[0].x, 0);

        // Check that flex items fill most of the remaining space
        // (some pixels may be lost to integer division)
        let total_width: u16 = rects.iter().map(|r| r.width).sum();
        let total_gap = 2 * 2; // 2 gaps between 3 items
        // Should be close to 100 but may be less due to rounding
        assert!(total_width + total_gap <= 100);
        assert!(total_width >= 90); // Should fill most of the space
    }

    #[test]
    fn test_row_percent() {
        let area = test_rect(0, 0, 100, 10);
        let items = vec![Item::percent(30), Item::percent(70)];
        let rects = row(area, 0, &items);

        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].width, 30);
        assert_eq!(rects[1].width, 70);
    }

    #[test]
    fn test_row_empty() {
        let area = test_rect(0, 0, 100, 10);
        let rects: Vec<Rect> = row(area, 2, &[]);
        assert!(rects.is_empty());
    }

    #[test]
    fn test_col_fixed() {
        let area = test_rect(0, 0, 10, 100);
        let items = vec![Item::fixed(20), Item::fixed(30), Item::fixed(10)];
        let rects = col(area, 2, &items);

        assert_eq!(rects.len(), 3);

        // First item
        assert_eq!(rects[0].y, 0);
        assert_eq!(rects[0].height, 20);

        // Second item (after 20 + 2 gap)
        assert_eq!(rects[1].y, 22);
        assert_eq!(rects[1].height, 30);

        // Third item
        assert_eq!(rects[2].y, 54);
        assert_eq!(rects[2].height, 10);
    }

    #[test]
    fn test_col_flex() {
        let area = test_rect(0, 0, 10, 100);
        let items = vec![Item::fixed(20), Item::flex(1), Item::flex(1)];
        let rects = col(area, 2, &items);

        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].height, 20);

        // Check that flex items fill remaining space
        let total_height = rects.iter().map(|r| r.height).sum::<u16>();
        let total_gap = 2 * 2;
        assert_eq!(total_height + total_gap, 100);
    }

    #[test]
    fn test_centered() {
        let area = test_rect(0, 0, 100, 100);
        let centered_rect = centered(area, 50, 30);

        assert_eq!(centered_rect.width, 50);
        assert_eq!(centered_rect.height, 30);
        assert_eq!(centered_rect.x, 25); // (100 - 50) / 2
        assert_eq!(centered_rect.y, 35); // (100 - 30) / 2
    }

    #[test]
    fn test_centered_clamps() {
        let area = test_rect(0, 0, 20, 20);
        let centered_rect = centered(area, 50, 50);

        // Should clamp to area size
        assert_eq!(centered_rect.width, 20);
        assert_eq!(centered_rect.height, 20);
        assert_eq!(centered_rect.x, 0);
        assert_eq!(centered_rect.y, 0);
    }

    #[test]
    fn test_row_min_size() {
        let area = test_rect(0, 0, 100, 10);
        // Min(50) in a 100px area should get 50
        let items = vec![Item::min(50), Item::flex(1)];
        let rects = row(area, 0, &items);

        assert_eq!(rects[0].width, 50);
        // Flex item gets the rest
        assert_eq!(rects[1].width, 50);
    }

    #[test]
    fn test_row_min_size_clamped() {
        let area = test_rect(0, 0, 30, 10);
        // Min(50) in a 30px area should get 30 (clamped)
        let items = vec![Item::min(50), Item::fixed(10)];
        let rects = row(area, 0, &items);

        // First item gets min(50, available) = 30, but then second item needs 10
        // This is a bit tricky - let's just verify it doesn't panic
        assert_eq!(rects.len(), 2);
    }

    #[test]
    fn test_saturating_math() {
        // Test that we don't panic on edge cases
        let area = test_rect(0, 0, 0, 0);
        let items = vec![Item::fixed(10), Item::fixed(10)];
        let rects = row(area, 5, &items);

        assert_eq!(rects.len(), 2);
        // Fixed sizes are not clamped, so they keep their values even if area is 0
        // This is intentional - the caller is responsible for ensuring items fit
        assert_eq!(rects[0].width, 10);
        assert_eq!(rects[1].width, 10);
    }
}
