//! Event-handling tests for TUI keyboard interactions.
//!
//! Responsibilities:
//! - Validate that key events mutate App state and queue order correctly.
//! - Cover command palette behavior and guardrails around actions.
//!
//! Not handled here:
//! - Rendering correctness or terminal backend integration.
//! - Runner execution side effects.
//!
//! Invariants/assumptions:
//! - Tests use deterministic in-memory queues and timestamps.
//! - Input events are synthetic and scoped to App state changes.

use super::types::ConfirmDiscardAction;
use super::*;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::tui::TextInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl_key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

fn input(value: &str) -> TextInput {
    TextInput::new(value)
}

fn input_with_cursor(value: &str, cursor: usize) -> TextInput {
    TextInput::from_parts(value, cursor)
}

fn assert_search_input(app: &App, expected: &str, cursor: usize) {
    match &app.mode {
        AppMode::Searching(query) => {
            assert_eq!(query.value(), expected);
            assert_eq!(query.cursor(), cursor);
        }
        other => panic!("expected search mode, got {:?}", other),
    }
}

#[test]
fn text_char_ignores_ctrl_and_alt_modifiers() {
    let plain = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(text_char(&plain), Some('a'));

    let shifted = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
    assert_eq!(text_char(&shifted), Some('A'));

    let ctrl = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(text_char(&ctrl), None);

    let alt = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
    assert_eq!(text_char(&alt), None);
}

fn make_test_task(id: &str) -> Task {
    Task {
        id: id.to_string(),
        title: "Test task".to_string(),
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at: None,
        depends_on: vec![],
        custom_fields: std::collections::HashMap::new(),
    }
}

#[test]
fn quit_when_not_running_exits_immediately() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('q')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Quit);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn quit_when_running_requires_confirmation() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.runner_active = true;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('q')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::ConfirmQuit);
}

#[test]
fn confirm_quit_accepts_yes() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmQuit;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Quit);
}

#[test]
fn unsafe_to_discard_detects_dirty_states() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    assert!(!app.unsafe_to_discard());

    app.dirty = true;
    assert!(app.unsafe_to_discard());
    app.dirty = false;

    app.dirty_done = true;
    assert!(app.unsafe_to_discard());
    app.dirty_done = false;

    app.dirty_config = true;
    assert!(app.unsafe_to_discard());
    app.dirty_config = false;

    app.save_error = Some("save failed".to_string());
    assert!(app.unsafe_to_discard());
}

#[test]
fn reload_when_dirty_requires_confirm_discard() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.dirty = true;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('r')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(
        app.mode,
        AppMode::ConfirmDiscard {
            action: ConfirmDiscardAction::ReloadQueue
        }
    );
}

#[test]
fn confirm_discard_reload_yes_triggers_reload() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDiscard {
        action: ConfirmDiscardAction::ReloadQueue,
    };

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::ReloadQueue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn confirm_discard_cancel_returns_to_normal() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDiscard {
        action: ConfirmDiscardAction::Quit,
    };

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-19T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn quit_when_dirty_requires_confirm_discard() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.dirty = true;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('q')),
        "2026-01-19T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(
        app.mode,
        AppMode::ConfirmDiscard {
            action: ConfirmDiscardAction::Quit
        }
    );
}

#[test]
fn palette_reload_with_save_error_requires_confirm_discard() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.save_error = Some("save failed".to_string());

    let action = app
        .execute_palette_command(PaletteCommand::ReloadQueue, "2026-01-19T00:00:00Z")
        .expect("execute command");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(
        app.mode,
        AppMode::ConfirmDiscard {
            action: ConfirmDiscardAction::ReloadQueue
        }
    );
}

#[test]
fn loop_key_starts_loop_and_runs_next_runnable() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('l')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::RunTask("RQ-0001".to_string()));
    assert!(app.loop_active);
    assert!(app.runner_active);
}

#[test]
fn delete_key_without_selection_sets_status_message() {
    let mut app = App::new(QueueFile::default());

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('d')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.status_message.as_deref(), Some("No task selected"));
}

#[test]
fn archive_flow_enters_confirm_mode_then_moves_tasks() {
    let mut done_task = make_test_task("RQ-0001");
    done_task.status = TaskStatus::Done;
    done_task.completed_at = Some("2026-01-19T00:00:00Z".to_string());

    let queue = QueueFile {
        version: 1,
        tasks: vec![done_task, make_test_task("RQ-0002")],
    };
    let mut app = App::new(queue);

    // Enter confirm archive.
    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('a')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::ConfirmArchive);

    // Confirm.
    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);

    assert_eq!(app.queue.tasks.len(), 1);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert_eq!(app.done.tasks.len(), 1);
    assert_eq!(app.done.tasks[0].id, "RQ-0001");
    assert!(app.dirty);
    assert!(app.dirty_done);
}

#[test]
fn colon_enters_command_palette() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char(':')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::CommandPalette { .. } => {}
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn colon_enters_command_palette_with_empty_queue() {
    let mut app = App::new(QueueFile::default());

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char(':')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::CommandPalette { .. } => {}
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn n_enters_create_mode_with_empty_queue() {
    let mut app = App::new(QueueFile::default());

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('n')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTask(input("")));
}

#[test]
fn help_key_enters_help_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('?')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn help_key_enters_help_mode_with_h() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('h')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn help_opens_from_search_and_returns_to_previous_mode() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Searching(input("needle"));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('?')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
    assert!(matches!(
        app.help_previous_mode(),
        Some(AppMode::Searching(query)) if query.value() == "needle"
    ));

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Searching(input("needle")));
}

#[test]
fn help_key_does_not_interrupt_search_input() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Searching(input(""));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('h')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Searching(input("h")));
}

#[test]
fn search_enter_applies_query_and_returns_to_normal() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Searching(input("needle"));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.filters.query, "needle");
}

#[test]
fn search_escape_cancels_without_applying_query() {
    let mut app = App::new(QueueFile::default());
    app.filters.query = "prior".to_string();
    app.mode = AppMode::Searching(input("new"));

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.filters.query, "prior");
}

#[test]
fn search_input_supports_cursor_edits_and_deletes() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Searching(input_with_cursor("ac", 1));

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('b')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_search_input(&app, "abc", 2);

    handle_key_event(&mut app, key_event(KeyCode::Left), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_search_input(&app, "abc", 1);

    handle_key_event(&mut app, key_event(KeyCode::Delete), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_search_input(&app, "ac", 1);

    handle_key_event(
        &mut app,
        key_event(KeyCode::Backspace),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_search_input(&app, "c", 0);
}

#[test]
fn search_ctrl_w_deletes_previous_word() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Searching(input("alpha beta"));

    handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('w')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_search_input(&app, "alpha ", 6);
}

#[test]
fn help_mode_closes_on_escape() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_mode_scrolls_and_clamps() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Help;
    app.set_help_visible_lines(3, 7);

    handle_key_event(&mut app, key_event(KeyCode::Down), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.help_scroll(), 1);

    handle_key_event(
        &mut app,
        key_event(KeyCode::PageDown),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(app.help_scroll(), 4);

    handle_key_event(
        &mut app,
        key_event(KeyCode::PageDown),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert_eq!(app.help_scroll(), 4);

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.help_scroll(), 0);

    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.help_scroll(), 4);
}

#[test]
fn normal_mode_home_end_jumps_list_selection_and_scroll() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![
            make_test_task("RQ-0001"),
            make_test_task("RQ-0002"),
            make_test_task("RQ-0003"),
            make_test_task("RQ-0004"),
            make_test_task("RQ-0005"),
        ],
    };
    let mut app = App::new(queue);
    app.list_height = 2;

    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.selected, 4);
    assert_eq!(app.scroll, 3);

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
}

#[test]
fn normal_mode_home_end_respects_filtered_list() {
    let mut task1 = make_test_task("RQ-0001");
    task1.tags = vec!["alpha".to_string()];
    let mut task2 = make_test_task("RQ-0002");
    task2.tags = vec!["beta".to_string()];
    let mut task3 = make_test_task("RQ-0003");
    task3.tags = vec!["alpha".to_string()];

    let queue = QueueFile {
        version: 1,
        tasks: vec![task1, task2, task3],
    };
    let mut app = App::new(queue);
    app.list_height = 1;
    app.set_tag_filters(vec!["alpha".to_string()]);

    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.selected, 1);
    assert_eq!(app.scroll, 1);
    assert_eq!(
        app.selected_task().map(|task| task.id.as_str()),
        Some("RQ-0003")
    );

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
    assert_eq!(
        app.selected_task().map(|task| task.id.as_str()),
        Some("RQ-0001")
    );
}

#[test]
fn normal_mode_home_end_scrolls_details_when_focused() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.focus_next_panel();
    app.details_visible_lines = 3;
    app.details_total_lines = 10;
    app.details_scroll = 2;
    let selected_before = app.selected;

    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.details_scroll, 7);
    assert_eq!(app.selected, selected_before);

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    assert_eq!(app.details_scroll, 0);
    assert_eq!(app.selected, selected_before);
}

#[test]
fn normal_mode_home_end_safe_with_empty_list() {
    let mut app = App::new(QueueFile::default());
    app.list_height = 3;

    handle_key_event(&mut app, key_event(KeyCode::Home), "2026-01-20T00:00:00Z")
        .expect("handle key");
    handle_key_event(&mut app, key_event(KeyCode::End), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll, 0);
}

#[test]
fn help_mode_closes_on_h() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('h')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_mode_closes_on_question_mark() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('?')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_mode_ignores_unrelated_keys() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('x')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn command_palette_runs_selected_command() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::CommandPalette {
        query: input("run selected"),
        selected: 0,
    };

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::RunTask("RQ-0001".to_string()));
    assert!(app.runner_active);
}

#[test]
fn command_palette_with_no_matches_sets_status_message() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::CommandPalette {
        query: input("nope"),
        selected: 0,
    };

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.status_message.as_deref(), Some("No matching command"));
}

#[test]
fn command_palette_typing_jk_appends_query_and_resets_selection() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CommandPalette {
        query: input("ru"),
        selected: 4,
    };

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('j')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "ruj");
            assert_eq!(*selected, 0);
        }
        other => panic!("expected command palette, got {:?}", other),
    }

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('k')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "rujk");
            assert_eq!(*selected, 0);
        }
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn command_palette_up_down_navigation_preserves_query() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CommandPalette {
        query: input("run"),
        selected: 1,
    };

    let action = handle_key_event(&mut app, key_event(KeyCode::Up), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "run");
            assert_eq!(*selected, 0);
        }
        other => panic!("expected command palette, got {:?}", other),
    }

    let action = handle_key_event(&mut app, key_event(KeyCode::Down), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "run");
            assert_eq!(*selected, 1);
        }
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn command_palette_ctrl_char_is_ignored_for_text_entry() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CommandPalette {
        query: input("run"),
        selected: 2,
    };

    let action = handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('j')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match &app.mode {
        AppMode::CommandPalette { query, selected } => {
            assert_eq!(query.value(), "run");
            assert_eq!(*selected, 2);
        }
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn c_enters_config_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('c')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::EditingConfig { .. } => {}
        other => panic!("expected config mode, got {:?}", other),
    }
}

#[test]
fn ctrl_c_quits_in_normal_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('c')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");

    assert_eq!(action, TuiAction::Quit);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ctrl_q_quits_in_normal_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('q')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");

    assert_eq!(action, TuiAction::Quit);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ctrl_p_enters_command_palette() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('p')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::CommandPalette { .. } => {}
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn ctrl_f_enters_search_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.filters.query = "needle".to_string();

    let action = handle_key_event(
        &mut app,
        ctrl_key_event(KeyCode::Char('f')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Searching(input("needle")));
}

#[test]
fn g_enters_scan_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('g')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::Scanning(_) => {}
        other => panic!("expected scan mode, got {:?}", other),
    }
}

#[test]
fn scan_mode_enter_runs_scan() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::Scanning(input("focus"));

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::RunScan("focus".to_string()));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn scan_mode_escape_cancels() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::Scanning(input("focus"));

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn scan_palette_command_enters_scan_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::CommandPalette {
        query: input("scan"),
        selected: 0,
    };

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::Scanning(_) => {}
        other => panic!("expected scan mode, got {:?}", other),
    }
}

#[test]
fn scan_rejected_when_runner_active() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.runner_active = true;
    app.mode = AppMode::Scanning(input("focus"));

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.status_message.as_deref(), Some("Runner already active"));
}

#[test]
fn config_mode_cycles_project_type() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::EditingConfig {
        selected: 0,
        editing_value: None,
    };

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(
        app.project_config.project_type,
        Some(crate::contracts::ProjectType::Code)
    );
    assert!(app.dirty_config);
}

#[test]
fn config_text_entry_updates_value() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    let idx = app
        .config_entries()
        .iter()
        .position(|entry| entry.key == crate::tui::ConfigKey::QueueIdPrefix)
        .expect("queue.id_prefix entry");
    app.mode = AppMode::EditingConfig {
        selected: idx,
        editing_value: None,
    };

    let _ = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('X')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");
    let _ =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(app.project_config.queue.id_prefix.as_deref(), Some("X"));
}

#[test]
fn uppercase_c_toggles_case_sensitive() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('C')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert!(app.filters.search_options.case_sensitive);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Case-sensitive search enabled")
    );
}

#[test]
fn uppercase_r_toggles_regex() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('R')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert!(app.filters.search_options.use_regex);
    assert_eq!(app.status_message.as_deref(), Some("Regex search enabled"));
}

#[test]
fn toggle_case_sensitive_twice_restores_default() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('C')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(app.filters.search_options.case_sensitive);

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('C')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(!app.filters.search_options.case_sensitive);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Case-sensitive search disabled")
    );
}

#[test]
fn toggle_regex_twice_restores_default() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('R')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(app.filters.search_options.use_regex);

    handle_key_event(
        &mut app,
        key_event(KeyCode::Char('R')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");
    assert!(!app.filters.search_options.use_regex);
    assert_eq!(app.status_message.as_deref(), Some("Regex search disabled"));
}

#[test]
fn palette_toggle_case_sensitive_command() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::ToggleCaseSensitive, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(app.filters.search_options.case_sensitive);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Case-sensitive search enabled")
    );
}

#[test]
fn palette_toggle_regex_command() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::ToggleRegex, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(app.filters.search_options.use_regex);
    assert_eq!(app.status_message.as_deref(), Some("Regex search enabled"));
}

#[test]
fn uppercase_o_enters_scope_filter_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('o')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(app.mode, AppMode::FilteringScopes(_)));
}

#[test]
fn palette_filter_scopes_command() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::FilterScopes, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(matches!(app.mode, AppMode::FilteringScopes(_)));
}

#[test]
fn enter_applies_scope_filter() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::FilteringScopes(input("crates/ralph"));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal, "mode should return to Normal");
    assert_eq!(
        app.filters.search_options.scopes,
        vec!["crates/ralph"],
        "scope filter should be applied"
    );
}

#[test]
fn uppercase_n_enters_task_builder_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('N')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(app.mode, AppMode::CreatingTaskDescription(_)));
}

#[test]
fn task_builder_mode_handles_character_input() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription(input(""));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('a')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTaskDescription(input("a")));
}

#[test]
fn task_builder_mode_handles_backspace() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription(input("ab"));

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Backspace),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTaskDescription(input("a")));
}

#[test]
fn task_builder_mode_escape_cancels() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription(input("test description"));

    let action = handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn task_builder_mode_empty_description_returns_to_normal() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription(input(""));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Description cannot be empty")
    );
}

#[test]
fn task_builder_mode_whitespace_only_returns_to_normal() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription(input("   "));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Description cannot be empty")
    );
}

#[test]
fn task_builder_mode_valid_description_builds_task() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription(input("Add a new feature"));

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(
        action,
        TuiAction::BuildTask("Add a new feature".to_string())
    );
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn palette_build_task_agent_command() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    app.execute_palette_command(PaletteCommand::BuildTaskAgent, "2026-01-20T00:00:00Z")
        .expect("execute command");

    assert!(matches!(app.mode, AppMode::CreatingTaskDescription(_)));
}

#[test]
fn uppercase_n_rejected_when_runner_active() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.runner_active = true;

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('N')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.status_message.as_deref(), Some("Runner already active"));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn uppercase_k_moves_selected_task_up_in_queue() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")],
    };
    let mut app = App::new(queue);
    app.selected = 1; // Select RQ-0002

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('K')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert_eq!(app.queue.tasks[1].id, "RQ-0001");
    assert!(app.dirty);
}

#[test]
fn uppercase_j_moves_selected_task_down_in_queue() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")],
    };
    let mut app = App::new(queue);
    app.selected = 0; // Select RQ-0001

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('J')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert_eq!(app.queue.tasks[1].id, "RQ-0001");
    assert!(app.dirty);
}

#[test]
fn move_task_up_at_top_does_not_mutate_queue() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")],
    };
    let mut app = App::new(queue);
    app.selected = 0; // Select RQ-0001

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('K')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0001");
    assert_eq!(app.queue.tasks[1].id, "RQ-0002");
    assert!(!app.dirty);
}

#[test]
fn move_task_down_at_bottom_does_not_mutate_queue() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")],
    };
    let mut app = App::new(queue);
    app.selected = 1; // Select RQ-0002

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('J')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0001");
    assert_eq!(app.queue.tasks[1].id, "RQ-0002");
    assert!(!app.dirty);
}

#[test]
fn move_task_with_filters_swaps_underlying_queue_indices() {
    let mut t1 = make_test_task("RQ-0001");
    t1.tags = vec!["a".to_string()];
    let t2 = make_test_task("RQ-0002"); // No tag 'a'
    let mut t3 = make_test_task("RQ-0003");
    t3.tags = vec!["a".to_string()];

    let queue = QueueFile {
        version: 1,
        tasks: vec![t1, t2, t3],
    };
    let mut app = App::new(queue);
    app.filters.tags = vec!["a".to_string()];
    app.rebuild_filtered_view();

    // Filtered list should be [RQ-0001, RQ-0003] (indices 0 and 2)
    assert_eq!(app.filtered_indices, vec![0, 2]);
    app.selected = 1; // Select RQ-0003

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('K')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    // Should swap RQ-0001 and RQ-0003 (indices 0 and 2)
    assert_eq!(app.queue.tasks[0].id, "RQ-0003");
    assert_eq!(app.queue.tasks[1].id, "RQ-0002"); // Unchanged
    assert_eq!(app.queue.tasks[2].id, "RQ-0001");
    assert!(app.dirty);
    assert_eq!(app.filtered_indices, vec![0, 2]);
    assert_eq!(
        app.selected_task().map(|task| task.id.as_str()),
        Some("RQ-0003")
    );
}

#[test]
fn command_palette_move_task_up_executes() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001"), make_test_task("RQ-0002")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::CommandPalette {
        query: input("move selected task up"),
        selected: 0,
    };
    app.selected = 1; // Select RQ-0002

    let action = handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z")
        .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert!(app.dirty);
}
