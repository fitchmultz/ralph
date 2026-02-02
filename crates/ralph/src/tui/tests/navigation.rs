//! Tests for task builder execution flow and navigation.
//!
//! Responsibilities:
//! - Validate task builder finish/error handling and mode transitions.
//! - Test queue reload behavior after external task creation.
//!
//! Not handled here:
//! - App state initialization, filtering, or phase tracking (see other modules).

use super::super::app::*;
use super::super::events::AppMode;
use super::{QueueFile, make_test_task};
use crate::contracts::TaskStatus;
use crate::queue;
use crate::tui::app_reload::ReloadOperations;
use tempfile::TempDir;

#[test]
fn task_builder_finish_reloads_queue_and_returns_to_normal() {
    let tmp = TempDir::new().expect("create temp dir");
    let queue_path = tmp.path().join("queue.json");
    let done_path = tmp.path().join("done.json");

    // Write initial queue
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    queue::save_queue(&queue_path, &initial_queue).expect("save initial queue");

    // Create app and set it to executing mode (like task builder would)
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Executing {
        task_id: "Task Builder".to_string(),
    };

    // Write updated queue with new task
    let updated_queue = QueueFile {
        version: 1,
        tasks: vec![
            make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
            make_test_task("RQ-0002", "New Task", TaskStatus::Todo),
        ],
    };
    queue::save_queue(&queue_path, &updated_queue).expect("save updated queue");

    // Simulate task builder finished
    app.on_task_builder_finished(&queue_path, &done_path);

    // Verify queue was reloaded
    assert_eq!(app.queue.tasks.len(), 2);
    assert_eq!(app.queue.tasks[1].id, "RQ-0002");

    // Verify mode returned to Normal
    assert_eq!(app.mode, AppMode::Normal);

    // Verify status message
    assert_eq!(
        app.status_message.as_deref(),
        Some("Task builder completed")
    );
}

#[test]
fn task_builder_error_sets_status_and_returns_to_normal() {
    let mut app = App::new(QueueFile::default());
    app.mode = AppMode::Executing {
        task_id: "Task Builder".to_string(),
    };

    app.on_task_builder_error("test error");

    // Verify error status message
    assert_eq!(
        app.status_message.as_deref(),
        Some("Task builder error: test error")
    );

    // Verify mode returned to Normal
    assert_eq!(app.mode, AppMode::Normal);
}
