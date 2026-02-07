//! Tests for App state management, task creation, editing, and persistence.
//!
//! Responsibilities:
//! - Validate App initialization, task creation, and editing behavior.
//! - Test runner error handling and auto-save functionality.
//! - Exercise config validation and state transitions.
//!
//! Not handled here:
//! - Terminal rendering, input polling, or cross-process execution.
//! - Filtering, palette fuzzy matching, or phase tracking (see other modules).

use super::super::app::*;
use super::super::config_edit::*;
use super::super::events::AppMode;
use super::{QueueFile, Result, canonical_rfc3339, make_test_task};
use crate::contracts::TaskStatus;
use crate::queue::TaskEditKey;
use crate::tui::app_session::auto_save_app_if_dirty;
use tempfile::TempDir;

#[test]
fn app_new_with_empty_queue() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let app = App::new(queue);
    assert_eq!(app.selected, 0);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.scroll, 0);
    assert!(!app.dirty);
    assert!(!app.runner_active);
}

#[test]
fn app_create_task_from_title_appends_with_defaults() -> Result<()> {
    let mut done_queue = QueueFile::default();
    let mut done_task = make_test_task("RQ-0005", "Done Task", TaskStatus::Done);
    done_task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    done_queue.tasks.push(done_task);

    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0003", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);
    app.id_prefix = "RQ".to_string();
    app.id_width = 4;
    app.done = done_queue;

    app.create_task_from_title("New Task", "2026-01-20T12:00:00Z")?;

    assert_eq!(app.queue.tasks.len(), 2);
    let task = &app.queue.tasks[1];
    assert_eq!(task.id, "RQ-0006");
    assert_eq!(task.title, "New Task");
    assert_eq!(task.status, TaskStatus::Todo);
    assert_eq!(task.priority, crate::contracts::TaskPriority::Medium);
    assert_eq!(task.created_at, Some("2026-01-20T12:00:00Z".to_string()));
    assert_eq!(task.updated_at, Some("2026-01-20T12:00:00Z".to_string()));
    assert!(task.completed_at.is_none());
    assert!(app.dirty);
    assert_eq!(app.mode, AppMode::Normal);
    Ok(())
}

#[test]
fn apply_task_edit_parses_list_fields() -> Result<()> {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);

    app.apply_task_edit(
        TaskEditKey::Tags,
        "alpha, beta,, gamma \n delta",
        "2026-01-20T12:00:00Z",
    )?;

    assert_eq!(
        app.queue.tasks[0].tags,
        vec![
            "alpha".to_string(),
            "beta".to_string(),
            "gamma".to_string(),
            "delta".to_string()
        ]
    );
    Ok(())
}

#[test]
fn apply_task_edit_cycles_status_with_policy() -> Result<()> {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Done)],
    };
    queue.tasks[0].completed_at = Some("2026-01-19T00:00:00Z".to_string());

    let mut app = App::new(queue);

    let now = "2026-01-20T12:00:00Z";
    let now_canon = canonical_rfc3339(now);
    app.apply_task_edit(TaskEditKey::Status, "", now)?;
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Rejected);
    assert_eq!(
        app.queue.tasks[0].completed_at.as_deref(),
        Some("2026-01-19T00:00:00Z")
    );
    assert_eq!(
        app.queue.tasks[0].updated_at.as_deref(),
        Some(now_canon.as_str())
    );

    let now2 = "2026-01-21T12:00:00Z";
    let now2_canon = canonical_rfc3339(now2);
    app.apply_task_edit(TaskEditKey::Status, "", now2)?;
    assert_eq!(app.queue.tasks[0].status, TaskStatus::Draft);
    assert!(app.queue.tasks[0].completed_at.is_none());
    assert_eq!(
        app.queue.tasks[0].updated_at.as_deref(),
        Some(now2_canon.as_str())
    );
    Ok(())
}

#[test]
fn apply_task_edit_custom_fields_parses_and_validates() -> Result<()> {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);

    app.apply_task_edit(
        TaskEditKey::CustomFields,
        "foo=bar, baz=qux",
        "2026-01-20T12:00:00Z",
    )?;
    assert_eq!(
        app.queue.tasks[0]
            .custom_fields
            .get("foo")
            .map(String::as_str),
        Some("bar")
    );
    assert_eq!(
        app.queue.tasks[0]
            .custom_fields
            .get("baz")
            .map(String::as_str),
        Some("qux")
    );

    let err = app
        .apply_task_edit(
            TaskEditKey::CustomFields,
            "bad key=value",
            "2026-01-20T12:10:00Z",
        )
        .expect_err("expected invalid custom field key");
    assert!(err.to_string().contains("whitespace"));
    assert_eq!(
        app.queue.tasks[0]
            .custom_fields
            .get("foo")
            .map(String::as_str),
        Some("bar")
    );
    Ok(())
}

#[test]
fn runner_error_summarizes_and_logs_details() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);

    app.set_runner_error("repo is dirty\n\nUse --force");

    assert_eq!(
        app.status_message.as_deref(),
        Some("Runner error: repo is dirty (see logs)")
    );
    assert_eq!(
        app.logs.first().map(String::as_str),
        Some("Runner error details:")
    );
    assert_eq!(app.logs.get(1).map(String::as_str), Some("repo is dirty"));
    assert_eq!(app.logs.get(2).map(String::as_str), Some(""));
    assert_eq!(app.logs.get(3).map(String::as_str), Some("Use --force"));
}

#[test]
fn runner_error_handles_empty_message() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);

    app.set_runner_error("   ");

    assert_eq!(
        app.status_message.as_deref(),
        Some("Runner error (see logs)")
    );
    assert_eq!(
        app.logs.first().map(String::as_str),
        Some("Runner error details:")
    );
    assert_eq!(
        app.logs.get(1).map(String::as_str),
        Some("(no details provided)")
    );
}

#[test]
fn apply_task_edit_clears_optional_field() -> Result<()> {
    let mut task = make_test_task("RQ-0001", "Task 1", TaskStatus::Todo);
    task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let mut app = App::new(queue);

    app.apply_task_edit(TaskEditKey::CompletedAt, "", "2026-01-20T12:00:00Z")?;
    assert!(app.queue.tasks[0].completed_at.is_none());
    Ok(())
}

#[test]
fn apply_task_edit_rejects_invalid_updated_at() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);

    let err = app
        .apply_task_edit(
            TaskEditKey::UpdatedAt,
            "not-a-timestamp",
            "2026-01-20T12:00:00Z",
        )
        .expect_err("expected invalid updated_at");
    assert!(err.to_string().contains("updated_at"));
}

#[test]
fn apply_task_edit_preserves_manual_updated_at() -> Result<()> {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);

    let updated_at = "2026-01-20T12:00:00Z";
    let updated_at_canon = canonical_rfc3339(updated_at);
    app.apply_task_edit(TaskEditKey::UpdatedAt, updated_at, "2026-01-22T12:00:00Z")?;

    assert_eq!(
        app.queue.tasks[0].updated_at.as_deref(),
        Some(updated_at_canon.as_str())
    );
    Ok(())
}

#[test]
fn apply_task_edit_rejects_invalid_dependency() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![
            make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
            make_test_task("RQ-0002", "Task 2", TaskStatus::Todo),
        ],
    };
    let mut app = App::new(queue);

    let err = app
        .apply_task_edit(TaskEditKey::DependsOn, "RQ-9999", "2026-01-20T12:00:00Z")
        .expect_err("expected invalid dependency");
    assert!(err.to_string().contains("Invalid dependency"));
    assert!(app.queue.tasks[0].depends_on.is_empty());
}

#[test]
fn auto_save_clears_dirty_on_success() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");
    let done_path = temp.path().join("done.json");
    let config_path = temp.path().join("config.json");

    let queue = QueueFile::default();
    let mut app = App::new(queue);
    app.dirty = true;
    app.dirty_done = true;
    app.dirty_config = true;
    app.project_config_path = Some(config_path.clone());

    auto_save_app_if_dirty(&mut app, &queue_path, &done_path, Some(&config_path));

    assert!(!app.dirty);
    assert!(!app.dirty_done);
    assert!(!app.dirty_config);
    assert!(app.save_error.is_none());
    assert!(queue_path.exists());
    assert!(done_path.exists());
    assert!(config_path.exists());
    Ok(())
}

#[test]
fn config_text_entry_rejects_invalid_id_width() {
    let mut app = App::new(QueueFile::default());
    let err = app
        .apply_config_text_value(ConfigKey::QueueIdWidth, "0")
        .expect_err("invalid id_width");
    assert!(err.to_string().contains("id_width"));
}
