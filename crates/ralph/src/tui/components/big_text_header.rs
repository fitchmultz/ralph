//! Big-text / ASCII header component for Ralph TUI.
//!
//! Responsibilities:
//! - Render a large ASCII header using `tui-big-text` with selectable fonts.
//! - Degrade gracefully to a plain text title on narrow terminals or failures.
//! - Provide a foundation `Component` wrapper so header rendering can be composed consistently.
//!
//! Not handled here:
//! - Persistent animation/state (belongs to animation utilities / App state).
//! - Multi-line layout orchestration beyond "render inside given `Rect`".
//!
//! Invariants/assumptions:
//! - Must never panic for any `Rect` size (including 0x0).
//! - Empty text must render nothing (or only fallback if provided).
//! - Fallback is always safe and deterministic.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::{
    App,
    foundation::{Component, ComponentId, FocusManager, RenderCtx, UiEvent},
};

// Keep crate usage isolated here so API mismatches are fixed in one file.
use tui_big_text::{BigText, PixelSize};

/// Font selection for big text headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BigHeaderFont {
    /// Full block font (largest, requires ~70+ columns).
    Block,
    /// Shade font using shaded blocks (requires ~50+ columns).
    Shade,
    /// Slick font (requires ~35+ columns).
    Slick,
    /// Tiny font (smallest, requires ~20+ columns).
    Tiny,
    /// Auto-select based on available width.
    Auto,
}

/// Configuration for big text header rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BigTextHeaderConfig {
    /// The text to render.
    pub(crate) text: String,
    /// The font to use (or Auto to select based on width).
    pub(crate) font: BigHeaderFont,
    /// Text alignment within the area.
    pub(crate) align: Alignment,
    /// Style to apply to the rendered text.
    pub(crate) style: Style,
    /// Fallback text to render if big text fails or is too narrow.
    /// If None, uses the main text as fallback.
    pub(crate) fallback_text: Option<String>,
    /// Minimum width required to attempt big text rendering.
    pub(crate) min_width_for_big: u16,
}

impl Default for BigTextHeaderConfig {
    fn default() -> Self {
        Self {
            text: String::new(),
            font: BigHeaderFont::Auto,
            align: Alignment::Center,
            style: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            fallback_text: None,
            min_width_for_big: 20,
        }
    }
}

/// Render-only header component (non-interactive).
pub(crate) struct BigTextHeaderComponent {
    id: ComponentId,
    cfg: BigTextHeaderConfig,
}

impl BigTextHeaderComponent {
    /// Create a new big text header component with the given text.
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            id: ComponentId::new("big_text_header", 0),
            cfg: BigTextHeaderConfig {
                text: text.into(),
                ..Default::default()
            },
        }
    }

    /// Set the entire configuration for this component.
    pub(crate) fn set_config(&mut self, cfg: BigTextHeaderConfig) {
        self.cfg = cfg;
    }

    /// Get a mutable reference to the configuration.
    pub(crate) fn config_mut(&mut self) -> &mut BigTextHeaderConfig {
        &mut self.cfg
    }

    /// Choose the appropriate font based on available width.
    fn choose_font(&self, width: u16) -> BigHeaderFont {
        match self.cfg.font {
            BigHeaderFont::Auto => {
                if width >= 70 {
                    BigHeaderFont::Block
                } else if width >= 50 {
                    BigHeaderFont::Shade
                } else if width >= 35 {
                    BigHeaderFont::Slick
                } else {
                    BigHeaderFont::Tiny
                }
            }
            other => other,
        }
    }

    /// Check if we should attempt big text rendering for the given area.
    fn should_use_big(&self, area: Rect) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }
        if self.cfg.text.trim().is_empty() {
            return false;
        }
        if area.width < self.cfg.min_width_for_big {
            return false;
        }
        true
    }

    /// Render the fallback text as a plain paragraph.
    fn render_fallback(&self, f: &mut Frame<'_>, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let Some(text) = self
            .cfg
            .fallback_text
            .as_deref()
            .or_else(|| (!self.cfg.text.trim().is_empty()).then_some(self.cfg.text.as_str()))
        else {
            return;
        };

        let p = Paragraph::new(Line::from(Span::styled(text.to_string(), self.cfg.style)))
            .alignment(self.cfg.align);

        f.render_widget(p, area);
    }

    /// Get the pixel size for the given font.
    fn pixel_size(&self, font: BigHeaderFont) -> PixelSize {
        match font {
            BigHeaderFont::Block => PixelSize::Full,
            BigHeaderFont::Shade => PixelSize::Quadrant,
            BigHeaderFont::Slick => PixelSize::HalfHeight,
            BigHeaderFont::Tiny | BigHeaderFont::Auto => PixelSize::HalfHeight,
        }
    }

    /// Build the BigText widget for the given width.
    fn build_big_text(&self, _width: u16) -> Option<BigText<'_>> {
        let font = self.choose_font(_width);
        let pixel_size = self.pixel_size(font);

        // Build the BigText widget
        // Note: tui-big-text uses a builder pattern with `lines()` taking Vec<Line>
        let lines: Vec<Line<'_>> = self.cfg.text.lines().map(|line| line.into()).collect();

        if lines.is_empty() {
            return None;
        }

        // Create and build the widget directly (build() returns BigText, not Result)
        let big_text = BigText::builder()
            .pixel_size(pixel_size)
            .style(self.cfg.style)
            .lines(lines)
            .build();

        Some(big_text)
    }

    /// Returns how many rows the header will consume (best-effort).
    pub(crate) fn measured_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        if !self.should_use_big(Rect::new(0, 0, width, 1)) {
            // Fallback is always 1 line
            return 1;
        }

        // Estimate height based on font
        let font = self.choose_font(width);
        let line_count = self.cfg.text.lines().count().max(1);

        // Convert pixel height to terminal rows (each row is 1 pixel in half-height, 2 in full)
        let rows_per_line = match font {
            BigHeaderFont::Block => 8,
            BigHeaderFont::Shade => 4,
            BigHeaderFont::Slick | BigHeaderFont::Tiny | BigHeaderFont::Auto => 2,
        };

        (line_count * rows_per_line).min(u16::MAX as usize) as u16
    }

    /// Non-component helper for immediate-mode renderers.
    pub(crate) fn render_into(&self, f: &mut Frame<'_>, area: Rect) {
        if !self.should_use_big(area) {
            self.render_fallback(f, area);
            return;
        }

        let Some(big_text) = self.build_big_text(area.width) else {
            self.render_fallback(f, area);
            return;
        };

        // Render the big text widget
        f.render_widget(big_text, area);
    }
}

impl Component for BigTextHeaderComponent {
    type Message = ();

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, _app: &App, _ctx: &mut RenderCtx<'_>) {
        self.render_into(f, area);
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
    use ratatui::layout::Rect;

    #[test]
    fn measured_height_is_zero_for_zero_width() {
        let c = BigTextHeaderComponent::new("RALPH");
        assert_eq!(c.measured_height(0), 0);
    }

    #[test]
    fn should_use_big_false_for_empty_text() {
        let c = BigTextHeaderComponent::new("   ");
        assert!(!c.should_use_big(Rect::new(0, 0, 80, 10)));
    }

    #[test]
    fn should_use_big_false_for_zero_area() {
        let c = BigTextHeaderComponent::new("RALPH");
        assert!(!c.should_use_big(Rect::new(0, 0, 0, 10)));
        assert!(!c.should_use_big(Rect::new(0, 0, 80, 0)));
        assert!(!c.should_use_big(Rect::new(0, 0, 0, 0)));
    }

    #[test]
    fn should_use_big_false_for_narrow_terminal() {
        let mut c = BigTextHeaderComponent::new("RALPH");
        c.cfg.min_width_for_big = 40;
        assert!(!c.should_use_big(Rect::new(0, 0, 30, 10)));
        assert!(c.should_use_big(Rect::new(0, 0, 50, 10)));
    }

    #[test]
    fn choose_font_auto_scales_with_width() {
        let mut c = BigTextHeaderComponent::new("RALPH");
        c.cfg.font = BigHeaderFont::Auto;
        assert_eq!(c.choose_font(80), BigHeaderFont::Block);
        assert_eq!(c.choose_font(55), BigHeaderFont::Shade);
        assert_eq!(c.choose_font(40), BigHeaderFont::Slick);
        assert_eq!(c.choose_font(10), BigHeaderFont::Tiny);
    }

    #[test]
    fn choose_font_respects_explicit_selection() {
        let mut c = BigTextHeaderComponent::new("RALPH");
        c.cfg.font = BigHeaderFont::Block;
        assert_eq!(c.choose_font(10), BigHeaderFont::Block); // Narrow but explicit

        c.cfg.font = BigHeaderFont::Tiny;
        assert_eq!(c.choose_font(100), BigHeaderFont::Tiny); // Wide but explicit
    }

    #[test]
    fn measured_height_returns_at_least_one_for_fallback() {
        let c = BigTextHeaderComponent::new("R");
        // Even with tiny width, if we can show fallback, height is 1
        assert_eq!(c.measured_height(10), 1);
    }

    #[test]
    fn config_mut_allows_modification() {
        let mut c = BigTextHeaderComponent::new("RALPH");
        c.config_mut().text = "TEST".to_string();
        c.config_mut().font = BigHeaderFont::Block;
        assert_eq!(c.cfg.text, "TEST");
        assert_eq!(c.cfg.font, BigHeaderFont::Block);
    }

    #[test]
    fn set_config_replaces_all() {
        let mut c = BigTextHeaderComponent::new("RALPH");
        let new_cfg = BigTextHeaderConfig {
            text: "NEW".to_string(),
            font: BigHeaderFont::Shade,
            align: Alignment::Left,
            style: Style::default().fg(Color::Red),
            fallback_text: Some("fallback".to_string()),
            min_width_for_big: 30,
        };
        c.set_config(new_cfg.clone());
        assert_eq!(c.cfg.text, "NEW");
        assert_eq!(c.cfg.font, BigHeaderFont::Shade);
        assert_eq!(c.cfg.align, Alignment::Left);
        assert_eq!(c.cfg.fallback_text, Some("fallback".to_string()));
        assert_eq!(c.cfg.min_width_for_big, 30);
    }
}
