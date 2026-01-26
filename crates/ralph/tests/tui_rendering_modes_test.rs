//! Rendering tests for modal/editor/dialog states.
//!
//! These tests validate overlays/dialogs that are controlled by `AppMode`
//! (task editor, create task prompt, confirm dialogs, revert dialog).

mod test_support;
mod tui_rendering_support;

use ralph::tui::{App, AppMode};
use test_support::make_render_test_queue as make_test_queue;
use tui_rendering_support::{get_rendered_output, setup_test_terminal};

#[test]
fn test_render_editing_task_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::EditingTask {
        selected: 0,
        editing_value: Some("Modified Title".to_string()),
    };
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Task Editor"));
    assert!(output.contains("title"));
    assert!(output.contains("Modified Title"));
}

#[test]
fn test_render_creating_task_mode_shows_prompt_and_title() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::CreatingTask("New Task".to_string());
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("New Task:"));
    assert!(output.contains("New Task"));
    assert!(output.contains("Status:"));
    assert!(output.contains("Priority:"));
}

#[test]
fn test_render_creating_task_mode_shows_placeholder_when_empty() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::CreatingTask(String::new());
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("(enter a title)"));
}

#[test]
fn test_render_confirm_delete_dialog() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDelete;
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Delete this task?"));
    assert!(output.contains("(y/n)"));
}

#[test]
fn test_render_confirm_quit_dialog() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmQuit;
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Task still running. Quit?"));
    assert!(output.contains("(y/n)"));
}

#[test]
fn test_render_confirm_revert_dialog() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let (tx, _rx) = std::sync::mpsc::channel();
    app.mode = AppMode::ConfirmRevert {
        label: "Phase 2 CI failure".to_string(),
        allow_proceed: false,
        selected: 0,
        input: String::new(),
        reply_sender: tx,
        previous_mode: Box::new(AppMode::Normal),
    };
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Phase 2 CI failure"));
    assert!(output.contains("Keep (default)"));
    assert!(output.contains("Revert"));
    assert!(output.contains("Other (type message)"));
}
