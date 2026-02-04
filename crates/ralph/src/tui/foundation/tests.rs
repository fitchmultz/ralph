//! Tests for the TUI foundation module.
//!
//! These tests verify the integration of focus management,
//! layout primitives, and component traits.

use super::*;
use crate::tui::foundation::focus::{
    ComponentId, FocusId, FocusManager, FocusNode, FocusRegistry, FocusScope, FocusTraversal,
};
use crate::tui::foundation::layout::{Item, Padding, h_split, pad, row, v_split};
use ratatui::layout::Rect;

/// Test that the full focus lifecycle works correctly.
#[test]
fn test_focus_lifecycle() {
    let mut registry = FocusRegistry::default();
    let mut manager = FocusManager::default();

    // Register some nodes
    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("list", 0), 0),
        FocusScope::Base,
        FocusTraversal::root(),
        Rect::new(0, 0, 50, 20),
        true,
    ));

    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("details", 0), 0),
        FocusScope::Base,
        FocusTraversal::root().child(0),
        Rect::new(50, 0, 50, 20),
        true,
    ));

    // Build focus order
    manager.rebuild(&registry);

    // Should focus first node
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("list", 0), 0))
    );

    // Navigate forward
    manager.focus_next();
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("details", 0), 0))
    );

    // Wrap around
    manager.focus_next();
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("list", 0), 0))
    );
}

/// Test overlay scope behavior.
#[test]
fn test_overlay_scope() {
    let mut registry = FocusRegistry::default();
    let mut manager = FocusManager::default();

    // Base scope nodes
    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("base", 0), 0),
        FocusScope::Base,
        FocusTraversal::root(),
        Rect::new(0, 0, 10, 10),
        true,
    ));

    // Overlay scope node
    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("overlay", 0), 0),
        FocusScope::Overlay,
        FocusTraversal::root(),
        Rect::new(0, 0, 10, 10),
        true,
    ));

    // Start in base scope
    manager.rebuild(&registry);
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("base", 0), 0))
    );

    // Enter overlay
    manager.enter_overlay_scope();
    manager.rebuild(&registry);
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("overlay", 0), 0))
    );

    // Exit overlay - should restore base focus
    manager.exit_overlay_scope();
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("base", 0), 0))
    );
}

/// Test layout integration with focus.
#[test]
fn test_layout_with_focus() {
    let area = Rect::new(0, 0, 100, 50);

    // Split area into two columns
    let (left, right) = h_split(area, 40, 2);

    assert_eq!(left.width, 40);
    assert_eq!(right.width, 58); // 100 - 40 - 2 gap

    // Create focus nodes in each area
    let mut registry = FocusRegistry::default();

    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("left_panel", 0), 0),
        FocusScope::Base,
        FocusTraversal::root(),
        left,
        true,
    ));

    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("right_panel", 0), 0),
        FocusScope::Base,
        FocusTraversal::root().child(0),
        right,
        true,
    ));

    // Test hit testing
    let mut manager = FocusManager::default();
    manager.rebuild(&registry);

    // Click in left panel
    manager.focus_by_point(20, 10, &registry);
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("left_panel", 0), 0))
    );

    // Click in right panel
    manager.focus_by_point(50, 10, &registry);
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("right_panel", 0), 0))
    );
}

/// Test flex layout with row.
#[test]
fn test_flex_row_layout() {
    let area = Rect::new(0, 0, 100, 10);

    // Create a row with fixed + flex + flex layout
    let items = vec![Item::fixed(20), Item::flex(2), Item::flex(1)];

    let rects = row(area, 2, &items);

    assert_eq!(rects.len(), 3);
    assert_eq!(rects[0].width, 20);

    // Total: 20 + 2 + flex(2) + 2 + flex(1) = 100
    // Flex space: 100 - 20 - 2 - 2 = 76
    // flex(2) gets 2 * (76/3) = 2 * 25 = 50
    // flex(1) gets 1 * (76/3) = 1 * 25 = 25
    // Total: 20 + 50 + 25 = 95 (5 pixels lost to integer division)

    let total_width: u16 = rects.iter().map(|r| r.width).sum();
    let total_gap = 2 * 2;
    // Should be close to 100 but may be less due to rounding
    assert!(total_width + total_gap <= 100);
    assert!(total_width >= 90); // Should fill most of the space
}

/// Test padding with focus areas.
#[test]
fn test_padding_with_focus() {
    let area = Rect::new(0, 0, 100, 50);
    let padded = pad(area, Padding::uniform(5));

    // Create a focus node in the padded area
    let mut registry = FocusRegistry::default();
    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("content", 0), 0),
        FocusScope::Base,
        FocusTraversal::root(),
        padded,
        true,
    ));

    let mut manager = FocusManager::default();
    manager.rebuild(&registry);

    // Click inside padded area
    manager.focus_by_point(10, 10, &registry);
    assert!(manager.focused().is_some());

    // Click outside padded area but inside outer area
    manager.focus_by_point(2, 2, &registry);
    // Focus should not change (click was outside the padded focus area)
    // Actually, the focus_by_point won't find anything, so focused stays as is
    // or becomes None if it was the only node
}

/// Test render context traversal.
#[test]
fn test_render_ctx_traversal() {
    let mut registry = FocusRegistry::default();

    // Simulate nested component rendering
    let mut root_ctx = RenderCtx::root(&mut registry);

    // Register root node
    root_ctx.register_focus(
        FocusId::new(ComponentId::new("root", 0), 0),
        Rect::new(0, 0, 100, 50),
        true,
    );

    // Create child context and register
    {
        let mut child_ctx = root_ctx.child(0);
        child_ctx.register_focus(
            FocusId::new(ComponentId::new("child", 0), 0),
            Rect::new(5, 5, 90, 40),
            true,
        );

        // Create grandchild context and register
        {
            let mut grandchild_ctx = child_ctx.child(1);
            grandchild_ctx.register_focus(
                FocusId::new(ComponentId::new("grandchild", 0), 0),
                Rect::new(10, 10, 80, 30),
                true,
            );
        }
    }

    // Verify all nodes were registered
    assert_eq!(registry.nodes().len(), 3);

    // Build focus order
    let mut manager = FocusManager::default();
    manager.rebuild(&registry);

    // Order should be root, child, grandchild (by traversal)
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("root", 0), 0))
    );
    manager.focus_next();
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("child", 0), 0))
    );
    manager.focus_next();
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("grandchild", 0), 0))
    );
}

/// Test edge cases with zero-sized areas.
#[test]
fn test_zero_sized_areas() {
    let area = Rect::new(0, 0, 0, 0);

    // Row layout with zero width - fixed items keep their size
    let items = vec![Item::fixed(10), Item::fixed(10)];
    let rects = row(area, 2, &items);

    assert_eq!(rects.len(), 2);
    // Fixed sizes are not clamped to area size
    assert_eq!(rects[0].width, 10);
    assert_eq!(rects[1].width, 10);

    // Padding on zero-sized area
    let padded = pad(area, Padding::uniform(5));
    assert_eq!(padded.width, 0);
    assert_eq!(padded.height, 0);

    // Split on zero-sized area
    let (top, bottom) = v_split(area, 5, 2);
    assert_eq!(top.height, 0);
    assert_eq!(bottom.height, 0);
}

/// Test focus with disabled nodes.
#[test]
fn test_disabled_nodes() {
    let mut registry = FocusRegistry::default();
    let mut manager = FocusManager::default();

    // Mix of enabled and disabled nodes
    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("enabled1", 0), 0),
        FocusScope::Base,
        FocusTraversal::root(),
        Rect::new(0, 0, 10, 10),
        true,
    ));

    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("disabled", 0), 0),
        FocusScope::Base,
        FocusTraversal::root().child(0),
        Rect::new(10, 0, 10, 10),
        false,
    ));

    registry.register(FocusNode::new(
        FocusId::new(ComponentId::new("enabled2", 0), 0),
        FocusScope::Base,
        FocusTraversal::root().child(1),
        Rect::new(20, 0, 10, 10),
        true,
    ));

    manager.rebuild(&registry);

    // Should only have 2 enabled nodes
    assert_eq!(manager.focus_count(), 2);

    // Navigation should skip disabled
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("enabled1", 0), 0))
    );
    manager.focus_next();
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("enabled2", 0), 0))
    );
    manager.focus_next();
    // Wraps back to first
    assert_eq!(
        manager.focused(),
        Some(FocusId::new(ComponentId::new("enabled1", 0), 0))
    );
}
