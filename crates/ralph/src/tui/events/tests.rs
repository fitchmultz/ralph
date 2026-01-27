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

use super::*;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('q'), "2026-01-19T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('q'), "2026-01-19T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('y'), "2026-01-19T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Quit);
}

#[test]
fn loop_key_starts_loop_and_runs_next_runnable() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action =
        handle_key_event(&mut app, KeyCode::Char('l'), "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::RunTask("RQ-0001".to_string()));
    assert!(app.loop_active);
    assert!(app.runner_active);
}

#[test]
fn delete_key_without_selection_sets_status_message() {
    let mut app = App::new(QueueFile::default());

    let action =
        handle_key_event(&mut app, KeyCode::Char('d'), "2026-01-20T00:00:00Z").expect("handle key");

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
    let action =
        handle_key_event(&mut app, KeyCode::Char('a'), "2026-01-20T00:00:00Z").expect("handle key");
    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::ConfirmArchive);

    // Confirm.
    let action =
        handle_key_event(&mut app, KeyCode::Char('y'), "2026-01-20T00:00:00Z").expect("handle key");
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

    let action =
        handle_key_event(&mut app, KeyCode::Char(':'), "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::CommandPalette { .. } => {}
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn colon_enters_command_palette_with_empty_queue() {
    let mut app = App::new(QueueFile::default());

    let action =
        handle_key_event(&mut app, KeyCode::Char(':'), "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::CommandPalette { .. } => {}
        other => panic!("expected command palette, got {:?}", other),
    }
}

#[test]
fn n_enters_create_mode_with_empty_queue() {
    let mut app = App::new(QueueFile::default());

    let action =
        handle_key_event(&mut app, KeyCode::Char('n'), "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTask(String::new()));
}

#[test]
fn help_key_enters_help_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action =
        handle_key_event(&mut app, KeyCode::Char('?'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('h'), "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn help_mode_closes_on_escape() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action =
        handle_key_event(&mut app, KeyCode::Esc, "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn help_mode_closes_on_h() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);
    app.mode = AppMode::Help;

    let action =
        handle_key_event(&mut app, KeyCode::Char('h'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('?'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('x'), "2026-01-20T00:00:00Z").expect("handle key");

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
        query: "run selected".to_string(),
        selected: 0,
    };

    let action =
        handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

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
        query: "nope".to_string(),
        selected: 0,
    };

    let action =
        handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.status_message.as_deref(), Some("No matching command"));
}

#[test]
fn c_enters_config_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action =
        handle_key_event(&mut app, KeyCode::Char('c'), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    match app.mode {
        AppMode::EditingConfig { .. } => {}
        other => panic!("expected config mode, got {:?}", other),
    }
}

#[test]
fn g_enters_scan_mode() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action =
        handle_key_event(&mut app, KeyCode::Char('g'), "2026-01-20T00:00:00Z").expect("key");

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
    app.mode = AppMode::Scanning("focus".to_string());

    let action = handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

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
    app.mode = AppMode::Scanning("focus".to_string());

    let action = handle_key_event(&mut app, KeyCode::Esc, "2026-01-20T00:00:00Z").expect("key");

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
        query: "scan".to_string(),
        selected: 0,
    };

    let action = handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

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
    app.mode = AppMode::Scanning("focus".to_string());

    let action = handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

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

    let action = handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

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

    let _ = handle_key_event(&mut app, KeyCode::Char('X'), "2026-01-20T00:00:00Z").expect("key");
    let _ = handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(app.project_config.queue.id_prefix.as_deref(), Some("X"));
}

#[test]
fn uppercase_c_toggles_case_sensitive() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001")],
    };
    let mut app = App::new(queue);

    let action =
        handle_key_event(&mut app, KeyCode::Char('C'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('R'), "2026-01-20T00:00:00Z").expect("handle key");

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

    handle_key_event(&mut app, KeyCode::Char('C'), "2026-01-20T00:00:00Z").expect("handle key");
    assert!(app.filters.search_options.case_sensitive);

    handle_key_event(&mut app, KeyCode::Char('C'), "2026-01-20T00:00:00Z").expect("handle key");
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

    handle_key_event(&mut app, KeyCode::Char('R'), "2026-01-20T00:00:00Z").expect("handle key");
    assert!(app.filters.search_options.use_regex);

    handle_key_event(&mut app, KeyCode::Char('R'), "2026-01-20T00:00:00Z").expect("handle key");
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

    let action =
        handle_key_event(&mut app, KeyCode::Char('o'), "2026-01-20T00:00:00Z").expect("handle key");

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
    app.mode = AppMode::FilteringScopes("crates/ralph".to_string());

    let action =
        handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('N'), "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert!(matches!(app.mode, AppMode::CreatingTaskDescription(_)));
}

#[test]
fn task_builder_mode_handles_character_input() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription(String::new());

    let action =
        handle_key_event(&mut app, KeyCode::Char('a'), "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTaskDescription("a".to_string()));
}

#[test]
fn task_builder_mode_handles_backspace() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription("ab".to_string());

    let action =
        handle_key_event(&mut app, KeyCode::Backspace, "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::CreatingTaskDescription("a".to_string()));
}

#[test]
fn task_builder_mode_escape_cancels() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription("test description".to_string());

    let action =
        handle_key_event(&mut app, KeyCode::Esc, "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn task_builder_mode_empty_description_returns_to_normal() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::CreatingTaskDescription(String::new());

    let action =
        handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

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
    app.mode = AppMode::CreatingTaskDescription("   ".to_string());

    let action =
        handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

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
    app.mode = AppMode::CreatingTaskDescription("Add a new feature".to_string());

    let action =
        handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('N'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('K'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('J'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('K'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('J'), "2026-01-20T00:00:00Z").expect("handle key");

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

    let action =
        handle_key_event(&mut app, KeyCode::Char('K'), "2026-01-20T00:00:00Z").expect("handle key");

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
        query: "move selected task up".to_string(),
        selected: 0,
    };
    app.selected = 1; // Select RQ-0002

    let action =
        handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.queue.tasks[0].id, "RQ-0002");
    assert!(app.dirty);
}
