//! Tests for configuration handling.
//!
//! Responsibilities:
//! - Test config mode entry.
//! - Test config value editing.
//! - Test risky config confirmation.
//! - Test keyboard shortcuts (ctrl+c, ctrl+q, etc.).
//! - Test scan mode.
//!
//! Does NOT handle:
//! - Other mode transitions (see modes.rs).
//! - Palette commands (see palette.rs).

use super::helpers::{ctrl_key_event, input, key_event, make_queue, make_test_task};
use crate::tui::events::handle_key_event;
use crate::tui::{App, AppMode, ConfigKey, TuiAction};
use crossterm::event::KeyCode;

#[test]
fn c_enters_config_mode() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::Scanning(input("focus"));

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::RunScan("focus".to_string()));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn scan_mode_escape_cancels() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    app.mode = AppMode::Scanning(input("focus"));

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn scan_palette_command_enters_scan_mode() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let idx = app
        .config_entries()
        .iter()
        .position(|entry| entry.key == ConfigKey::QueueIdPrefix)
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('R')),
        "2026-01-20T00:00:00Z",
    )
    .expect("handle key");

    assert_eq!(action, TuiAction::Continue);
    assert!(app.filters.search_options.use_regex);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Regex search enabled (fuzzy disabled)")
    );
}

#[test]
fn toggle_case_sensitive_twice_restores_default() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
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
fn risky_config_enabling_git_push_shows_confirmation() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    // Find the git_commit_push_enabled entry index
    let idx = app
        .config_entries()
        .iter()
        .position(|entry| matches!(entry.key, ConfigKey::AgentGitCommitPushEnabled))
        .expect("git_commit_push_enabled entry");
    app.mode = AppMode::EditingConfig {
        selected: idx,
        editing_value: None,
    };
    // Ensure git_commit_push_enabled is not already enabled
    app.project_config.agent.git_commit_push_enabled = None;

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    // Should enter ConfirmRiskyConfig mode, not apply the change immediately
    assert!(
        matches!(app.mode, AppMode::ConfirmRiskyConfig { .. }),
        "expected ConfirmRiskyConfig mode, got {:?}",
        app.mode
    );
    // Config should not be marked dirty yet
    assert!(!app.dirty_config);
}

#[test]
fn risky_config_confirm_yes_applies_change() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let idx = app
        .config_entries()
        .iter()
        .position(|entry| matches!(entry.key, ConfigKey::AgentGitCommitPushEnabled))
        .expect("git_commit_push_enabled entry");
    app.mode = AppMode::ConfirmRiskyConfig {
        key: ConfigKey::AgentGitCommitPushEnabled,
        new_value: "true".to_string(),
        warning: "Test warning".to_string(),
        previous_mode: Box::new(AppMode::EditingConfig {
            selected: idx,
            editing_value: None,
        }),
    };

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('y')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");

    assert_eq!(action, TuiAction::Continue);
    assert_eq!(app.project_config.agent.git_commit_push_enabled, Some(true));
    assert!(app.dirty_config);
    assert!(matches!(app.mode, AppMode::EditingConfig { .. }));
}

#[test]
fn risky_config_confirm_no_cancels_change() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let idx = app
        .config_entries()
        .iter()
        .position(|entry| matches!(entry.key, ConfigKey::AgentGitCommitPushEnabled))
        .expect("git_commit_push_enabled entry");
    app.mode = AppMode::ConfirmRiskyConfig {
        key: ConfigKey::AgentGitCommitPushEnabled,
        new_value: "true".to_string(),
        warning: "Test warning".to_string(),
        previous_mode: Box::new(AppMode::EditingConfig {
            selected: idx,
            editing_value: None,
        }),
    };
    // Pre-set the config value to ensure it doesn't change
    app.project_config.agent.git_commit_push_enabled = Some(false);

    let action = handle_key_event(
        &mut app,
        key_event(KeyCode::Char('n')),
        "2026-01-20T00:00:00Z",
    )
    .expect("key");

    assert_eq!(action, TuiAction::Continue);
    // Config should remain unchanged
    assert_eq!(
        app.project_config.agent.git_commit_push_enabled,
        Some(false)
    );
    assert!(!app.dirty_config);
    assert!(matches!(app.mode, AppMode::EditingConfig { .. }));
}

#[test]
fn risky_config_confirm_esc_cancels_change() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let idx = app
        .config_entries()
        .iter()
        .position(|entry| matches!(entry.key, ConfigKey::AgentGitCommitPushEnabled))
        .expect("git_commit_push_enabled entry");
    app.mode = AppMode::ConfirmRiskyConfig {
        key: ConfigKey::AgentGitCommitPushEnabled,
        new_value: "true".to_string(),
        warning: "Test warning".to_string(),
        previous_mode: Box::new(AppMode::EditingConfig {
            selected: idx,
            editing_value: None,
        }),
    };
    app.project_config.agent.git_commit_push_enabled = Some(false);

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Esc), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    // Config should remain unchanged
    assert_eq!(
        app.project_config.agent.git_commit_push_enabled,
        Some(false)
    );
    assert!(!app.dirty_config);
    assert!(matches!(app.mode, AppMode::EditingConfig { .. }));
}

#[test]
fn risky_config_already_enabled_skips_confirmation() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    let idx = app
        .config_entries()
        .iter()
        .position(|entry| matches!(entry.key, ConfigKey::AgentGitCommitPushEnabled))
        .expect("git_commit_push_enabled entry");
    app.mode = AppMode::EditingConfig {
        selected: idx,
        editing_value: None,
    };
    // Pre-enable git_commit_push_enabled - cycling should go to false without confirmation
    app.project_config.agent.git_commit_push_enabled = Some(true);

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    // Should apply immediately since we're disabling, not enabling
    assert!(
        !matches!(app.mode, AppMode::ConfirmRiskyConfig { .. }),
        "should not enter ConfirmRiskyConfig when disabling"
    );
    // Value should be cycled to false
    assert_eq!(
        app.project_config.agent.git_commit_push_enabled,
        Some(false)
    );
}

#[test]
fn warning_level_config_no_confirmation() {
    let queue = make_queue(vec![make_test_task("RQ-0001")]);
    let mut app = App::new(queue);
    // Find approval_mode entry (Warning level, not Danger)
    let idx = app
        .config_entries()
        .iter()
        .position(|entry| matches!(entry.key, ConfigKey::AgentRunnerCliApprovalMode))
        .expect("approval_mode entry");
    app.mode = AppMode::EditingConfig {
        selected: idx,
        editing_value: None,
    };

    let action =
        handle_key_event(&mut app, key_event(KeyCode::Enter), "2026-01-20T00:00:00Z").expect("key");

    assert_eq!(action, TuiAction::Continue);
    // Should apply immediately without confirmation (Warning level, not Danger)
    assert!(
        !matches!(app.mode, AppMode::ConfirmRiskyConfig { .. }),
        "Warning level should not trigger confirmation"
    );
    assert!(app.dirty_config);
}
