//! Rendering tests for the footer help and help overlay.
//!
//! Responsibilities:
//! - Validate footer hints and help overlay text for keymap-driven content.
//! - Exercise mode-specific footer rendering output.
//!
//! Not handled here:
//! - Event handling behavior or keymap definitions.
//! - Visual styling correctness beyond text presence.
//!
//! Invariants/assumptions:
//! - Tests operate on deterministic TestBackend output.
//! - Assertions rely on ASCII text in the rendered buffer.

mod test_support;
mod tui_rendering_support;

use ralph::tui::ConfirmDiscardAction;
use ralph::tui::{App, AppMode, MultiLineInput, TextInput};
use test_support::make_render_test_queue as make_test_queue;
use tui_rendering_support::{get_rendered_output, setup_test_terminal};

#[test]
fn test_render_help_footer_normal_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(160, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Check for keybinding hints (help footer may be truncated on narrow terminals)
    // Core navigation and actions
    assert!(output.contains("?/h:help"));
    assert!(output.contains("q:quit"));
    assert!(output.contains(":nav"));
    assert!(output.contains(":run"));
    assert!(output.contains(":del"));
    assert!(output.contains(":edit"));
    // Filter/search functionality - check for at least one of these
    let has_filter_features = output.contains("search")
        || output.contains("tags")
        || output.contains("filter")
        || output.contains("clear")
        || output.contains("/:search")
        || output.contains("t:tags")
        || output.contains("f:filter")
        || output.contains("x:clear");
    assert!(
        has_filter_features,
        "Help footer should include filter/search keybindings"
    );
}

#[test]
fn test_render_help_footer_editing_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some(MultiLineInput::new("test", false)),
    };
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Enter:save"));
    assert!(output.contains("Esc:cancel"));
}

#[test]
fn test_render_help_footer_creating_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::CreatingTask(TextInput::new("new"));
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Enter:create"));
    assert!(output.contains("Esc:cancel"));
}

#[test]
fn test_render_help_footer_confirm_delete_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDelete;
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("y:yes"));
    assert!(output.contains("n:no"));
    assert!(output.contains("Esc:cancel"));
}

#[test]
fn test_render_help_footer_confirm_quit_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmQuit;
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("y:quit"));
    assert!(output.contains("n:stay"));
    assert!(output.contains("Esc:cancel"));
}

#[test]
fn test_render_help_footer_confirm_discard_reload_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDiscard {
        action: ConfirmDiscardAction::ReloadQueue,
    };
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("y:reload"));
    assert!(output.contains("n:cancel"));
    assert!(output.contains("Esc:cancel"));
}

#[test]
fn test_render_help_footer_confirm_discard_quit_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDiscard {
        action: ConfirmDiscardAction::Quit,
    };
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("y:quit"));
    assert!(output.contains("n:cancel"));
    assert!(output.contains("Esc:cancel"));
}

#[test]
fn test_render_help_footer_executing_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // In executing mode, the help footer is in the execution view title
    assert!(output.contains("Esc to return"));
}

#[test]
fn test_render_help_overlay() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Help;
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Help"));
    assert!(output.contains("Keybindings"));
    assert!(output.contains("Navigation"));
    assert!(output.contains("Esc:close"));
}
