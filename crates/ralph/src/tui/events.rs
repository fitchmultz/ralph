//! TUI event handling extracted from `crate::tui`.
//!
//! This module contains all key-event dispatch and per-mode handlers.
//! Public API is preserved via `crate::tui` re-exporting:
//! - `AppMode`
//! - `TuiAction`
//! - `handle_key_event`
//!
//! This is a pure refactor: behavior must remain identical to the prior
//! inline implementation in `tui.rs`.

use anyhow::Result;
use crossterm::event::KeyCode;

use super::App;

/// Actions that can result from handling a key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    /// Continue running the TUI
    Continue,
    /// Exit the TUI
    Quit,
    /// Run a specific task (transitions to Executing mode)
    RunTask(String),
}

/// Interaction modes for the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    /// Normal navigation mode
    Normal,
    /// Editing task title
    EditingTitle(String),
    /// Confirming task deletion
    ConfirmDelete,
    /// Confirming quit while a task is running
    ConfirmQuit,
    /// Executing a task (live output view)
    Executing { task_id: String },
}

/// Handle a key event and return the resulting action.
///
/// This function is the core of TUI interaction handling and is public
/// to allow testing without a full terminal setup.
pub fn handle_key_event(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match app.mode.clone() {
        AppMode::Normal => handle_normal_mode_key(app, key, now_rfc3339),
        AppMode::EditingTitle(ref current) => {
            handle_editing_mode_key(app, key, current, now_rfc3339)
        }
        AppMode::ConfirmDelete => handle_confirm_delete_key(app, key),
        AppMode::ConfirmQuit => handle_confirm_quit_key(app, key),
        AppMode::Executing { .. } => handle_executing_mode_key(app, key),
    }
}

/// Handle key events in Normal mode.
fn handle_normal_mode_key(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => {
            if app.runner_active {
                app.mode = AppMode::ConfirmQuit;
                Ok(TuiAction::Continue)
            } else {
                Ok(TuiAction::Quit)
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_up();
            Ok(TuiAction::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let list_height = app.list_height;
            app.move_down(list_height);
            Ok(TuiAction::Continue)
        }
        KeyCode::Enter => {
            if let Some(task) = app.selected_task() {
                let task_id = task.id.clone();
                app.mode = AppMode::Executing {
                    task_id: task_id.clone(),
                };
                app.logs.clear();
                app.log_scroll = 0;
                app.autoscroll = true;
                app.runner_active = true;
                Ok(TuiAction::RunTask(task_id))
            } else {
                Ok(TuiAction::Continue)
            }
        }
        KeyCode::Char('d') => {
            if app.selected_task().is_some() {
                app.mode = AppMode::ConfirmDelete;
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('e') => {
            if let Some(task) = app.selected_task() {
                app.mode = AppMode::EditingTitle(task.title.clone());
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('s') => {
            if let Err(e) = app.cycle_status(now_rfc3339) {
                app.logs.push(format!("Error: {}", e));
            }
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in EditingTitle mode.
fn handle_editing_mode_key(
    app: &mut App,
    key: KeyCode,
    current: &str,
    now_rfc3339: &str,
) -> Result<TuiAction> {
    match key {
        KeyCode::Enter => {
            let new_title = current.to_string();
            if let Err(e) = app.update_title(new_title, now_rfc3339) {
                app.logs.push(format!("Error: {}", e));
            } else {
                app.mode = AppMode::Normal;
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(c) => {
            let mut new_title = current.to_string();
            new_title.push(c);
            app.mode = AppMode::EditingTitle(new_title);
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut new_title = current.to_string();
            new_title.pop();
            app.mode = AppMode::EditingTitle(new_title);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmDelete mode.
fn handle_confirm_delete_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Err(e) = app.delete_selected_task() {
                app.logs.push(format!("Error: {}", e));
            }
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmQuit mode.
fn handle_confirm_quit_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => Ok(TuiAction::Quit),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in Executing mode.
fn handle_executing_mode_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    let visible_lines = app.log_visible_lines();
    let page_lines = visible_lines.saturating_sub(1).max(1);
    match key {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.scroll_logs_up(1);
            Ok(TuiAction::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.scroll_logs_down(1, visible_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::PageUp => {
            app.scroll_logs_up(page_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::PageDown => {
            app.scroll_logs_down(page_lines, visible_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('a') => {
            if app.autoscroll {
                app.autoscroll = false;
            } else {
                app.enable_autoscroll(visible_lines);
            }
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

#[cfg(test)]
mod tests {
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

        let action = handle_key_event(&mut app, KeyCode::Char('q'), "2026-01-19T00:00:00Z")
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

        let action = handle_key_event(&mut app, KeyCode::Char('q'), "2026-01-19T00:00:00Z")
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

        let action = handle_key_event(&mut app, KeyCode::Char('y'), "2026-01-19T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Quit);
    }

    #[test]
    fn confirm_quit_cancels_on_no() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::ConfirmQuit;

        let action = handle_key_event(&mut app, KeyCode::Char('n'), "2026-01-19T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn run_task_sets_runner_active() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);

        let action =
            handle_key_event(&mut app, KeyCode::Enter, "2026-01-19T00:00:00Z").expect("handle key");

        assert_eq!(action, TuiAction::RunTask("RQ-0001".to_string()));
        assert!(app.runner_active);
        assert_eq!(
            app.mode,
            AppMode::Executing {
                task_id: "RQ-0001".to_string()
            }
        );
    }

    #[test]
    fn executing_mode_scroll_up_disables_autoscroll() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Executing {
            task_id: "RQ-0001".to_string(),
        };
        app.logs = (0..40).map(|i| format!("line {}", i)).collect();
        app.log_scroll = 5;
        app.autoscroll = true;

        let action =
            handle_key_event(&mut app, KeyCode::Up, "2026-01-19T00:00:00Z").expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.log_scroll, 4);
        assert!(!app.autoscroll);
    }

    #[test]
    fn executing_mode_page_down_clamps_at_end() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Executing {
            task_id: "RQ-0001".to_string(),
        };
        app.logs = (0..50).map(|i| format!("line {}", i)).collect();
        app.log_scroll = 0;
        app.autoscroll = false;

        handle_key_event(&mut app, KeyCode::PageDown, "2026-01-19T00:00:00Z").expect("handle key");
        assert_eq!(app.log_scroll, 19);

        handle_key_event(&mut app, KeyCode::PageDown, "2026-01-19T00:00:00Z").expect("handle key");
        assert_eq!(app.log_scroll, 30);

        handle_key_event(&mut app, KeyCode::PageDown, "2026-01-19T00:00:00Z").expect("handle key");
        assert_eq!(app.log_scroll, 30);
    }

    #[test]
    fn executing_mode_toggle_autoscroll_jumps_to_bottom() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Executing {
            task_id: "RQ-0001".to_string(),
        };
        app.logs = (0..50).map(|i| format!("line {}", i)).collect();
        app.log_scroll = 5;
        app.autoscroll = false;

        handle_key_event(&mut app, KeyCode::Char('a'), "2026-01-19T00:00:00Z").expect("handle key");

        assert!(app.autoscroll);
        assert_eq!(app.log_scroll, 30);

        handle_key_event(&mut app, KeyCode::Char('a'), "2026-01-19T00:00:00Z").expect("handle key");
        assert!(!app.autoscroll);
        assert_eq!(app.log_scroll, 30);
    }
}
