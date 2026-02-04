//! Focus management for the Ralph TUI foundation.
//!
//! Responsibilities:
//! - Collect focusable nodes each frame (id + rect + order key + scope).
//! - Compute deterministic focus order within the active scope.
//! - Provide wraparound navigation (next/prev) and mouse hit-testing.
//!
//! Not handled here:
//! - Rendering focus styles (components decide how focused state looks).
//! - Persisting focus across app restarts.
//!
//! Invariants/assumptions:
//! - Focus order must not depend on registration order (uses traversal path).
//! - Focus must not panic on tiny terminal sizes (0x0, 0 width/height rects).
//! - Scopes isolate focus sets: overlays trap focus until closed.
//! - Wraparound cycles through the focus ring deterministically.

use ratatui::layout::Rect;

/// Unique identifier for a component instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ComponentId {
    /// Component type/kind (static string for stable ordering).
    pub(crate) kind: &'static str,
    /// Instance index (0 for singletons, stable index for repeated instances).
    pub(crate) instance: u32,
}

impl ComponentId {
    /// Create a new component ID.
    pub(crate) const fn new(kind: &'static str, instance: u32) -> Self {
        Self { kind, instance }
    }
}

/// Unique identifier for a focusable node within a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FocusId {
    /// The component containing this focusable node.
    pub(crate) component: ComponentId,
    /// Local index within the component (for components with multiple focusables).
    pub(crate) local: u16,
}

impl FocusId {
    /// Create a new focus ID.
    pub(crate) const fn new(component: ComponentId, local: u16) -> Self {
        Self { component, local }
    }
}

/// Traversal path for deterministic focus ordering.
///
/// This is a path from the root to the component, where each element
/// is a child index at that level. This ensures focus order is stable
/// regardless of registration order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) struct FocusTraversal {
    /// Path indices from root to this node.
    path: Vec<u16>,
}

impl FocusTraversal {
    /// Create a root traversal path.
    pub(crate) fn root() -> Self {
        Self { path: Vec::new() }
    }

    /// Create a child traversal path by appending an index.
    pub(crate) fn child(&self, index: u16) -> Self {
        let mut path = self.path.clone();
        path.push(index);
        Self { path }
    }

    /// Get the path as a slice.
    pub(crate) fn as_slice(&self) -> &[u16] {
        &self.path
    }
}

/// Focus scope determines which set of focusable nodes are active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FocusScope {
    /// Base UI scope (list, details panels, etc.).
    #[default]
    Base,
    /// Overlay scope (modals, editors, palettes) - traps focus.
    Overlay,
}

/// A focusable node registered during rendering.
#[derive(Debug, Clone)]
pub(crate) struct FocusNode {
    /// Unique identifier for this focusable.
    pub(crate) id: FocusId,
    /// Which scope this node belongs to.
    pub(crate) scope: FocusScope,
    /// Traversal path for deterministic ordering.
    pub(crate) traversal: FocusTraversal,
    /// Screen rectangle for hit-testing.
    pub(crate) rect: Rect,
    /// Whether this node can currently receive focus.
    pub(crate) enabled: bool,
}

impl FocusNode {
    /// Create a new focus node.
    pub(crate) fn new(
        id: FocusId,
        scope: FocusScope,
        traversal: FocusTraversal,
        rect: Rect,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            scope,
            traversal,
            rect,
            enabled,
        }
    }
}

/// Registry for collecting focusable nodes during rendering.
///
/// This is populated during each render pass and then used to
/// rebuild the focus manager's ordering.
#[derive(Debug, Default)]
pub(crate) struct FocusRegistry {
    nodes: Vec<FocusNode>,
}

impl FocusRegistry {
    /// Clear all registered nodes (call at start of render).
    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Register a focusable node.
    pub(crate) fn register(&mut self, node: FocusNode) {
        self.nodes.push(node);
    }

    /// Get all registered nodes.
    pub(crate) fn nodes(&self) -> &[FocusNode] {
        &self.nodes
    }
}

/// Manages focus state: active scope, focus ring, and navigation.
#[derive(Debug, Default)]
pub(crate) struct FocusManager {
    /// Current active scope.
    active_scope: FocusScope,
    /// Computed focus order for the active scope.
    order: Vec<FocusId>,
    /// Currently focused node (if any).
    focused: Option<FocusId>,
    /// Remembered focus when entering overlay scope.
    remembered_base_focus: Option<FocusId>,
}

impl FocusManager {
    /// Get the current active scope.
    #[cfg(test)]
    pub(crate) fn active_scope(&self) -> FocusScope {
        self.active_scope
    }

    /// Get the currently focused node ID.
    pub(crate) fn focused(&self) -> Option<FocusId> {
        self.focused
    }

    /// Check if a specific node is currently focused.
    pub(crate) fn is_focused(&self, id: FocusId) -> bool {
        self.focused == Some(id)
    }

    /// Enter overlay scope, remembering base focus.
    pub(crate) fn enter_overlay_scope(&mut self) {
        if self.active_scope != FocusScope::Overlay {
            self.remembered_base_focus = self.focused;
            self.active_scope = FocusScope::Overlay;
            self.order.clear();
            self.focused = None;
        }
    }

    /// Exit overlay scope, restoring base focus if possible.
    pub(crate) fn exit_overlay_scope(&mut self) {
        if self.active_scope != FocusScope::Base {
            self.active_scope = FocusScope::Base;
            self.order.clear();
            self.focused = self.remembered_base_focus;
        }
    }

    /// Rebuild the focus order from the registry.
    ///
    /// This filters by scope and enabled state, then sorts by
    /// traversal path for deterministic ordering.
    pub(crate) fn rebuild(&mut self, registry: &FocusRegistry) {
        let mut nodes: Vec<&FocusNode> = registry
            .nodes()
            .iter()
            .filter(|n| n.enabled && n.scope == self.active_scope)
            .collect();

        // Sort by traversal path, then by ID for deterministic ordering
        nodes.sort_by(|a, b| a.traversal.cmp(&b.traversal).then_with(|| a.id.cmp(&b.id)));

        self.order = nodes.into_iter().map(|n| n.id).collect();

        // Ensure focused is valid; fall back to first if needed
        if let Some(focused) = self.focused {
            if !self.order.contains(&focused) {
                self.focused = self.order.first().copied();
            }
        } else {
            self.focused = self.order.first().copied();
        }
    }

    /// Move focus to the next node (wraps around).
    pub(crate) fn focus_next(&mut self) {
        if self.order.is_empty() {
            self.focused = None;
            return;
        }

        let next = match self.focused {
            None => self.order[0],
            Some(current) => {
                let pos = self.order.iter().position(|id| *id == current).unwrap_or(0);
                self.order[(pos + 1) % self.order.len()]
            }
        };
        self.focused = Some(next);
    }

    /// Move focus to the previous node (wraps around).
    pub(crate) fn focus_prev(&mut self) {
        if self.order.is_empty() {
            self.focused = None;
            return;
        }

        let prev = match self.focused {
            None => self.order[0],
            Some(current) => {
                let pos = self.order.iter().position(|id| *id == current).unwrap_or(0);
                self.order[(pos + self.order.len() - 1) % self.order.len()]
            }
        };
        self.focused = Some(prev);
    }

    /// Focus the node at the given point (mouse hit-testing).
    pub(crate) fn focus_by_point(&mut self, x: u16, y: u16, registry: &FocusRegistry) {
        let mut hits: Vec<&FocusNode> = registry
            .nodes()
            .iter()
            .filter(|n| n.enabled && n.scope == self.active_scope && rect_contains(n.rect, x, y))
            .collect();

        // Sort by traversal + id for deterministic selection
        hits.sort_by(|a, b| a.traversal.cmp(&b.traversal).then_with(|| a.id.cmp(&b.id)));

        if let Some(node) = hits.first() {
            self.focused = Some(node.id);
        }
    }

    /// Get the number of focusable nodes in the current scope.
    #[cfg(test)]
    pub(crate) fn focus_count(&self) -> usize {
        self.order.len()
    }

    /// Explicitly set focus to a specific node ID.
    ///
    /// Notes:
    /// - Does not validate membership in the current ring until the next `rebuild()`.
    /// - Safe to call even when order is empty.
    pub(crate) fn focus(&mut self, id: FocusId) {
        self.focused = Some(id);
    }

    /// Clear focus (next rebuild will pick first focusable if any).
    pub(crate) fn clear_focus(&mut self) {
        self.focused = None;
    }
}

/// Check if a point is inside a rectangle.
fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x.saturating_add(r.width) && y >= r.y && y < r.y.saturating_add(r.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rect(x: u16, y: u16, w: u16, h: u16) -> Rect {
        Rect::new(x, y, w, h)
    }

    fn test_focus_id(kind: &'static str, instance: u32, local: u16) -> FocusId {
        FocusId::new(ComponentId::new(kind, instance), local)
    }

    #[test]
    fn test_focus_traversal_ordering() {
        let root = FocusTraversal::root();
        let child_0 = root.child(0);
        let child_1 = root.child(1);
        let grandchild = child_0.child(0);

        // Root comes before children
        assert!(root < child_0);
        // Child 0 comes before child 1
        assert!(child_0 < child_1);
        // Grandchild comes after its parent
        assert!(child_0 < grandchild);
    }

    #[test]
    fn test_focus_registry_clear() {
        let mut registry = FocusRegistry::default();
        let node = FocusNode::new(
            test_focus_id("test", 0, 0),
            FocusScope::Base,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        );
        registry.register(node);
        assert_eq!(registry.nodes().len(), 1);

        registry.clear();
        assert!(registry.nodes().is_empty());
    }

    #[test]
    fn test_focus_manager_rebuild_filters_scope() {
        let mut manager = FocusManager::default();
        let mut registry = FocusRegistry::default();

        // Add base scope node
        registry.register(FocusNode::new(
            test_focus_id("base", 0, 0),
            FocusScope::Base,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        ));

        // Add overlay scope node
        registry.register(FocusNode::new(
            test_focus_id("overlay", 0, 0),
            FocusScope::Overlay,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        ));

        // In base scope, only base node should be in order
        manager.rebuild(&registry);
        assert_eq!(manager.focus_count(), 1);
        assert_eq!(manager.focused(), Some(test_focus_id("base", 0, 0)));

        // Enter overlay scope
        manager.enter_overlay_scope();
        manager.rebuild(&registry);
        assert_eq!(manager.focus_count(), 1);
        assert_eq!(manager.focused(), Some(test_focus_id("overlay", 0, 0)));
    }

    #[test]
    fn test_focus_manager_rebuild_filters_disabled() {
        let mut manager = FocusManager::default();
        let mut registry = FocusRegistry::default();

        registry.register(FocusNode::new(
            test_focus_id("enabled", 0, 0),
            FocusScope::Base,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        ));

        registry.register(FocusNode::new(
            test_focus_id("disabled", 0, 0),
            FocusScope::Base,
            FocusTraversal::root().child(0),
            test_rect(0, 0, 10, 10),
            false,
        ));

        manager.rebuild(&registry);
        assert_eq!(manager.focus_count(), 1);
        assert_eq!(manager.focused(), Some(test_focus_id("enabled", 0, 0)));
    }

    #[test]
    fn test_focus_manager_wraparound_next() {
        let mut manager = FocusManager::default();
        let mut registry = FocusRegistry::default();

        registry.register(FocusNode::new(
            test_focus_id("first", 0, 0),
            FocusScope::Base,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        ));

        registry.register(FocusNode::new(
            test_focus_id("second", 0, 0),
            FocusScope::Base,
            FocusTraversal::root().child(0),
            test_rect(0, 0, 10, 10),
            true,
        ));

        manager.rebuild(&registry);
        assert_eq!(manager.focused(), Some(test_focus_id("first", 0, 0)));

        manager.focus_next();
        assert_eq!(manager.focused(), Some(test_focus_id("second", 0, 0)));

        // Wrap around to first
        manager.focus_next();
        assert_eq!(manager.focused(), Some(test_focus_id("first", 0, 0)));
    }

    #[test]
    fn test_focus_manager_wraparound_prev() {
        let mut manager = FocusManager::default();
        let mut registry = FocusRegistry::default();

        registry.register(FocusNode::new(
            test_focus_id("first", 0, 0),
            FocusScope::Base,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        ));

        registry.register(FocusNode::new(
            test_focus_id("second", 0, 0),
            FocusScope::Base,
            FocusTraversal::root().child(0),
            test_rect(0, 0, 10, 10),
            true,
        ));

        manager.rebuild(&registry);
        manager.focus_next(); // Move to second
        assert_eq!(manager.focused(), Some(test_focus_id("second", 0, 0)));

        manager.focus_prev();
        assert_eq!(manager.focused(), Some(test_focus_id("first", 0, 0)));

        // Wrap around to second
        manager.focus_prev();
        assert_eq!(manager.focused(), Some(test_focus_id("second", 0, 0)));
    }

    #[test]
    fn test_focus_manager_empty_order() {
        let mut manager = FocusManager::default();
        let registry = FocusRegistry::default();

        manager.rebuild(&registry);
        assert_eq!(manager.focus_count(), 0);
        assert_eq!(manager.focused(), None);

        // Should not panic
        manager.focus_next();
        manager.focus_prev();
        assert_eq!(manager.focused(), None);
    }

    #[test]
    fn test_focus_manager_scope_memory() {
        let mut manager = FocusManager::default();
        let mut registry = FocusRegistry::default();

        registry.register(FocusNode::new(
            test_focus_id("base", 0, 0),
            FocusScope::Base,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        ));

        registry.register(FocusNode::new(
            test_focus_id("overlay", 0, 0),
            FocusScope::Overlay,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        ));

        manager.rebuild(&registry);
        assert_eq!(manager.focused(), Some(test_focus_id("base", 0, 0)));

        manager.enter_overlay_scope();
        manager.rebuild(&registry);
        assert_eq!(manager.focused(), Some(test_focus_id("overlay", 0, 0)));

        manager.exit_overlay_scope();
        assert_eq!(manager.focused(), Some(test_focus_id("base", 0, 0)));
    }

    #[test]
    fn test_focus_manager_hit_testing() {
        let mut manager = FocusManager::default();
        let mut registry = FocusRegistry::default();

        registry.register(FocusNode::new(
            test_focus_id("left", 0, 0),
            FocusScope::Base,
            FocusTraversal::root(),
            test_rect(0, 0, 10, 10),
            true,
        ));

        registry.register(FocusNode::new(
            test_focus_id("right", 0, 0),
            FocusScope::Base,
            FocusTraversal::root().child(0),
            test_rect(10, 0, 10, 10),
            true,
        ));

        manager.rebuild(&registry);

        // Click on left
        manager.focus_by_point(5, 5, &registry);
        assert_eq!(manager.focused(), Some(test_focus_id("left", 0, 0)));

        // Click on right
        manager.focus_by_point(15, 5, &registry);
        assert_eq!(manager.focused(), Some(test_focus_id("right", 0, 0)));

        // Click outside
        manager.focus_by_point(100, 100, &registry);
        // Focus should remain unchanged
        assert_eq!(manager.focused(), Some(test_focus_id("right", 0, 0)));
    }

    #[test]
    fn test_focus_manager_deterministic_order() {
        // Test that focus order is deterministic regardless of registration order
        let mut manager = FocusManager::default();
        let mut registry = FocusRegistry::default();

        // Register in "wrong" order
        registry.register(FocusNode::new(
            test_focus_id("second", 0, 0),
            FocusScope::Base,
            FocusTraversal::root().child(1),
            test_rect(10, 0, 10, 10),
            true,
        ));

        registry.register(FocusNode::new(
            test_focus_id("first", 0, 0),
            FocusScope::Base,
            FocusTraversal::root().child(0),
            test_rect(0, 0, 10, 10),
            true,
        ));

        manager.rebuild(&registry);

        // Should still be in traversal order
        assert_eq!(manager.focused(), Some(test_focus_id("first", 0, 0)));
        manager.focus_next();
        assert_eq!(manager.focused(), Some(test_focus_id("second", 0, 0)));
    }

    #[test]
    fn test_rect_contains_edge_cases() {
        let rect = test_rect(5, 5, 10, 10);

        // Inside
        assert!(rect_contains(rect, 5, 5));
        assert!(rect_contains(rect, 14, 14));

        // Outside (exclusive bounds)
        assert!(!rect_contains(rect, 15, 5));
        assert!(!rect_contains(rect, 5, 15));

        // Zero-sized rect
        let zero = test_rect(5, 5, 0, 0);
        assert!(!rect_contains(zero, 5, 5));
    }

    #[test]
    fn test_focus_manager_set_focus_then_rebuild_keeps_if_present() {
        let mut manager = FocusManager::default();
        let mut registry = FocusRegistry::default();

        let a = test_focus_id("a", 0, 0);
        let b = test_focus_id("b", 0, 0);

        registry.register(FocusNode::new(
            a,
            FocusScope::Base,
            FocusTraversal::root(),
            test_rect(0, 0, 1, 1),
            true,
        ));
        registry.register(FocusNode::new(
            b,
            FocusScope::Base,
            FocusTraversal::root().child(0),
            test_rect(1, 0, 1, 1),
            true,
        ));

        manager.rebuild(&registry);
        manager.focus(b);
        manager.rebuild(&registry);

        assert_eq!(manager.focused(), Some(b));
    }
}
