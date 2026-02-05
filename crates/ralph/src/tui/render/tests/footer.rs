//! Tests for footer rendering.
//!
//! Responsibilities:
//! - Validate footer content including hints and error indicators.
//!
//! Not handled here:
//! - Header or panel rendering.
//! - Overlay rendering.

use super::common::*;
use crate::contracts::QueueFile;
use crate::tui::App;

#[test]
fn help_footer_includes_save_error_indicator() {
    let mut app = App::new(QueueFile::default());
    app.save_error = Some("failed to save".to_string());

    let rendered = footer_text(&app, 160);

    assert!(rendered.contains("SAVE ERROR"));
}

#[test]
fn help_footer_includes_config_hint() {
    let app = App::new(QueueFile::default());
    let rendered = footer_text(&app, 160);

    assert!(rendered.contains(":config"));
}

#[test]
fn help_footer_includes_scan_hint() {
    let app = App::new(QueueFile::default());
    let rendered = footer_text(&app, 160);

    assert!(rendered.contains(":scan"));
}

#[test]
fn help_footer_excludes_save_error_when_none() {
    let mut app = App::new(QueueFile::default());
    app.save_error = None;

    let rendered = footer_text(&app, 160);

    assert!(!rendered.contains("SAVE ERROR"));
}

#[test]
fn help_footer_includes_keymap_shortcuts_in_normal_mode() {
    let app = App::new(QueueFile::default());
    // Width increased to 280 to accommodate the new P (parallel) keybinding
    let rendered = footer_text(&app, 280);

    for expected in ["K/J", "Ctrl+P", "Ctrl+F", ":scope", ":case", ":regex"] {
        assert!(
            rendered.contains(expected),
            "missing footer hint: {expected}"
        );
    }
}

#[test]
fn help_footer_truncates_with_ellipsis_on_small_width() {
    let app = App::new(QueueFile::default());
    let rendered = footer_text(&app, 12);

    assert!(rendered.contains("..."));
}
