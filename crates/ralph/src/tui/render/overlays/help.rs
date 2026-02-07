//! Help overlay rendering.
//!
//! Responsibilities:
//! - Render full-screen help overlay with keybindings and scrollable content.
//! - Display scroll indicators when content exceeds visible area.
//! - Show a big "RALPH" header when terminal is wide enough.
//!
//! Not handled here:
//! - Help content generation (handled by `super::super::help`).
//! - Event handling for scrolling (handled by app event loop).
//!
//! Invariants/assumptions:
//! - Callers provide a properly sized terminal area.
//! - App state tracks help scroll position via `set_help_visible_lines`.

use crate::tui::App;
use crate::tui::app_resize::ResizeOperations;
use crate::tui::app_scroll::ScrollOperations;
use crate::tui::components::animation::{AnimationPolicy, FadeIn};
use crate::tui::components::big_text_header::{
    BigHeaderFont, BigTextHeaderComponent, BigTextHeaderConfig,
};
use crate::tui::components::markdown_renderer::{MarkdownRenderConfig, MarkdownRenderer};
use crate::tui::help;
use crate::tui::render::utils::scroll_indicator;
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Draw full-screen help overlay with keybindings.
pub fn draw_help_overlay(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let popup = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    f.render_widget(Clear, popup);

    let inner = popup.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    // --- Animation state for overlay fade-in ---
    let policy = AnimationPolicy::from_env();
    let now = app.ui_frame();
    let start = app.help_overlay_start_frame(now);
    let fade = FadeIn::new(start, 8);

    // --- Big header configuration ---
    let mut header = BigTextHeaderComponent::new("RALPH");
    let mut hcfg = BigTextHeaderConfig::default();
    hcfg.text = "RALPH".to_string();
    hcfg.font = BigHeaderFont::Auto;
    hcfg.fallback_text = Some("RALPH".to_string());
    hcfg.min_width_for_big = 22;
    hcfg.style = fade.overlay_style(hcfg.style, now, policy);
    header.set_config(hcfg);

    // Compute header height; keep a 1-row gap if possible.
    let header_h = header.measured_height(inner.width).min(inner.height);
    let gap = if inner.height > header_h { 1 } else { 0 };

    let header_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: header_h,
    };
    let content_area = Rect {
        x: inner.x,
        y: inner.y.saturating_add(header_h).saturating_add(gap),
        width: inner.width,
        height: inner.height.saturating_sub(header_h).saturating_sub(gap),
    };

    // Render big header if there's space
    if header_area.height > 0 {
        header.render_into(f, header_area);
    }

    let content_width = content_area.width as usize;

    // Render help as Markdown
    let md = help::help_overlay_markdown();
    let cfg = MarkdownRenderConfig::new(content_width);
    let lines = MarkdownRenderer::render(&md, cfg);
    let total_lines = lines.len();

    let visible_lines = content_area.height as usize;
    app.set_help_visible_lines(visible_lines, total_lines);

    let indicator = scroll_indicator(app.help_scroll(), app.help_visible_lines(), total_lines);

    // Fade overlay title/border subtly.
    let title_style =
        fade.overlay_style(Style::default().add_modifier(Modifier::BOLD), now, policy);
    let border_style = fade.overlay_style(Style::default().fg(Color::Gray), now, policy);
    let block = Block::default()
        .title(help_title(indicator, title_style))
        .title_style(title_style)
        .borders(Borders::ALL)
        .border_style(border_style);
    f.render_widget(block, popup);

    if content_area.width == 0 || content_area.height == 0 {
        return;
    }
    let paragraph = Paragraph::new(Text::from(lines)).scroll((app.help_scroll() as u16, 0));
    f.render_widget(paragraph, content_area);
}

fn help_title(indicator: Option<String>, title_style: Style) -> Line<'static> {
    let mut spans = vec![Span::styled("Help", title_style)];
    if let Some(indicator) = indicator {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            indicator,
            Style::default().fg(ratatui::style::Color::DarkGray),
        ));
    }
    Line::from(spans)
}
