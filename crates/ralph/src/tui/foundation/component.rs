//! Component trait and render/event contexts for the Ralph TUI foundation.
//!
//! Responsibilities:
//! - Define the core component interface used by migrated screens/overlays.
//! - Provide render context plumbing (focus registry + traversal path).
//!
//! Not handled here:
//! - AppMode dispatch (still owned by existing `tui/events/*`).
//! - Global app state ownership (still owned by `App`).
//!
//! Invariants/assumptions:
//! - Rendering is immediate-mode; components must be safe to render every frame.
//! - Focus registration happens during render to ensure rects are current.
//! - Components can be nested; child components receive updated traversal paths.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::event::UiEvent;
use super::focus::{FocusId, FocusManager, FocusNode, FocusRegistry, FocusScope, FocusTraversal};
use crate::tui::App;

/// Context passed to components during rendering.
///
/// This provides access to the focus registry and traversal path
/// for deterministic focus ordering.
pub(crate) struct RenderCtx<'a> {
    /// Registry for focusable nodes.
    pub(crate) focus: &'a mut FocusRegistry,
    /// Current focus scope.
    pub(crate) scope: FocusScope,
    /// Current traversal path.
    pub(crate) traversal: FocusTraversal,
}

impl<'a> RenderCtx<'a> {
    /// Create a new render context.
    pub(crate) fn new(
        focus: &'a mut FocusRegistry,
        scope: FocusScope,
        traversal: FocusTraversal,
    ) -> Self {
        Self {
            focus,
            scope,
            traversal,
        }
    }

    /// Create a root render context for the base scope.
    pub(crate) fn root(focus: &'a mut FocusRegistry) -> Self {
        Self::new(focus, FocusScope::Base, FocusTraversal::root())
    }

    /// Create a root render context for an overlay scope.
    pub(crate) fn overlay(focus: &'a mut FocusRegistry) -> Self {
        Self::new(focus, FocusScope::Overlay, FocusTraversal::root())
    }

    /// Create a child context with an appended traversal index.
    pub(crate) fn child(&mut self, index: u16) -> RenderCtx<'_> {
        RenderCtx {
            focus: self.focus,
            scope: self.scope,
            traversal: self.traversal.child(index),
        }
    }

    /// Register a focusable node in the current context.
    ///
    /// The node will be registered with the current scope and traversal path.
    pub(crate) fn register_focus(&mut self, id: FocusId, rect: Rect, enabled: bool) {
        self.focus.register(FocusNode::new(
            id,
            self.scope,
            self.traversal.clone(),
            rect,
            enabled,
        ));
    }
}

/// Core component trait for the TUI foundation.
///
/// Components are stateful structs that can render themselves and handle events.
/// They integrate with the focus management system for keyboard navigation.
///
/// # Type Parameters
///
/// Components are generic over the message type they produce. This allows
/// components to communicate with their parent without tight coupling.
pub(crate) trait Component {
    /// The message type this component produces when handling events.
    type Message;

    /// Render the component to the frame.
    ///
    /// # Arguments
    ///
    /// * `f` - The frame to render to
    /// * `area` - The area available for rendering
    /// * `app` - The application state
    /// * `ctx` - The render context for focus registration
    fn render(&mut self, f: &mut Frame<'_>, area: Rect, app: &App, ctx: &mut RenderCtx<'_>);

    /// Handle an event and optionally produce a message.
    ///
    /// Returns `Some(Message)` if the event was consumed, `None` if it was not.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to handle
    /// * `app` - The application state
    /// * `focus` - The focus manager for checking/setting focus
    fn handle_event(
        &mut self,
        event: &UiEvent,
        app: &App,
        focus: &mut FocusManager,
    ) -> Option<Self::Message>;

    /// Called when the component gains focus.
    fn focus_gained(&mut self) {}

    /// Called when the component loses focus.
    fn focus_lost(&mut self) {}

    /// Check if this component is currently focused.
    fn is_focused(&self, focus: &FocusManager, id: FocusId) -> bool {
        focus.is_focused(id)
    }
}

/// A boxed component that can be stored in App.
///
/// This allows storing different component types in the same field
/// without making App generic over component types.
pub(crate) type BoxedComponent<M> = Box<dyn Component<Message = M>>;

/// Helper trait for components that can be converted to boxed components.
pub(crate) trait IntoBoxedComponent<M>: Component<Message = M> + Sized + 'static {
    fn boxed(self) -> BoxedComponent<M> {
        Box::new(self)
    }
}

impl<M, T: Component<Message = M> + Sized + 'static> IntoBoxedComponent<M> for T {}

/// A simple component that wraps a render function.
///
/// This is useful for simple components that don't need complex state
/// or event handling.
pub(crate) struct SimpleComponent<F> {
    render_fn: F,
}

impl<F> SimpleComponent<F> {
    /// Create a new simple component with the given render function.
    pub(crate) fn new(render_fn: F) -> Self {
        Self { render_fn }
    }
}

impl<F> Component for SimpleComponent<F>
where
    F: FnMut(&mut Frame<'_>, Rect, &App, &mut RenderCtx<'_>),
{
    type Message = ();

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, app: &App, ctx: &mut RenderCtx<'_>) {
        (self.render_fn)(f, area, app, ctx);
    }

    fn handle_event(
        &mut self,
        _event: &UiEvent,
        _app: &App,
        _focus: &mut FocusManager,
    ) -> Option<Self::Message> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::foundation::focus::{ComponentId, FocusId};

    #[test]
    fn test_render_ctx_root() {
        let mut registry = FocusRegistry::default();
        let ctx = RenderCtx::root(&mut registry);

        assert!(matches!(ctx.scope, FocusScope::Base));
        assert!(ctx.traversal.as_slice().is_empty());
    }

    #[test]
    fn test_render_ctx_overlay() {
        let mut registry = FocusRegistry::default();
        let ctx = RenderCtx::overlay(&mut registry);

        assert!(matches!(ctx.scope, FocusScope::Overlay));
    }

    #[test]
    fn test_render_ctx_child() {
        let mut registry = FocusRegistry::default();
        let mut ctx = RenderCtx::root(&mut registry);
        let mut child = ctx.child(0);

        assert_eq!(child.traversal.as_slice(), &[0u16]);

        let grandchild = child.child(1);
        assert_eq!(grandchild.traversal.as_slice(), &[0u16, 1u16]);
    }

    #[test]
    fn test_render_ctx_register_focus() {
        let mut registry = FocusRegistry::default();
        let mut ctx = RenderCtx::root(&mut registry);

        let id = FocusId::new(ComponentId::new("test", 0), 0);
        let rect = Rect::new(0, 0, 10, 10);
        ctx.register_focus(id, rect, true);

        assert_eq!(registry.nodes().len(), 1);
        assert_eq!(registry.nodes()[0].id, id);
        assert_eq!(registry.nodes()[0].rect, rect);
    }

    #[test]
    fn test_simple_component() {
        // Verify SimpleComponent implements Component trait
        // We can't easily test rendering without a real app state,
        // so we just verify the types compile correctly
        fn _assert_component_trait() {
            fn check_render_fn(
                _f: &mut ratatui::Frame<'_>,
                _area: Rect,
                _app: &App,
                _ctx: &mut RenderCtx<'_>,
            ) {
            }
            let _component = SimpleComponent::new(check_render_fn);
            // The type implements Component<Message = ()>
        }
    }
}
