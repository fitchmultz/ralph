//! Unit tests for the TUI application state.
//!
//! Responsibilities:
//! - Validate App behavior around queue edits, filtering, and status updates.
//! - Exercise filter summaries and in-memory state transitions.
//!
//! Not handled here:
//! - Terminal rendering, input polling, or cross-process execution.
//! - Persistence/locking integration beyond in-memory helpers.
//!
//! Invariants/assumptions:
//! - Tests operate on in-memory queues with deterministic timestamps.
//! - File IO uses temporary directories for isolation.

use super::app::*;
use super::config_edit::*;
use super::events::{AppMode, PaletteCommand};
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue::{self, TaskEditKey};
use crate::timeutil;
use anyhow::Result;
use tempfile::TempDir;

fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        title: title.to_string(),
        status,
        priority: TaskPriority::Medium,
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test evidence".to_string()],
        plan: vec!["test plan".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at: None,
        depends_on: vec![],
        custom_fields: std::collections::HashMap::new(),
    }
}

fn canonical_rfc3339(ts: &str) -> String {
    let dt = timeutil::parse_rfc3339(ts).expect("valid RFC3339 timestamp");
    timeutil::format_rfc3339(dt).expect("format RFC3339 timestamp")
}

fn make_test_task_with_tags(id: &str, title: &str, tags: Vec<&str>) -> Task {
    let mut task = make_test_task(id, title, TaskStatus::Todo);
    task.tags = tags.into_iter().map(|tag| tag.to_string()).collect();
    task
}

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
    assert_eq!(task.priority, TaskPriority::Medium);
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

    auto_save_if_dirty(&mut app, &queue_path, &done_path, Some(&config_path));

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

#[test]
fn app_filters_by_tags() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![
            make_test_task_with_tags("RQ-0001", "UI polish", vec!["ux", "tui"]),
            make_test_task_with_tags("RQ-0002", "Docs", vec!["docs"]),
        ],
    };
    let mut app = App::new(queue);
    app.set_tag_filters(vec!["tui".to_string()]);

    assert_eq!(app.filtered_len(), 1);
    assert_eq!(
        app.selected_task().map(|task| task.id.as_str()),
        Some("RQ-0001")
    );
}

#[test]
fn rebuild_filtered_view_cache_hits_without_changes() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);
    let baseline = app.filter_cache_stats();

    app.rebuild_filtered_view();

    let after = app.filter_cache_stats();
    assert_eq!(after.id_index_rebuilds, baseline.id_index_rebuilds);
    assert_eq!(after.filtered_rebuilds, baseline.filtered_rebuilds);
}

#[test]
fn rebuild_filtered_view_rebuilds_on_filter_change() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);
    let baseline = app.filter_cache_stats();

    app.set_search_query("task".to_string());

    let after = app.filter_cache_stats();
    assert_eq!(after.filtered_rebuilds, baseline.filtered_rebuilds + 1);
}

#[test]
fn rebuild_filtered_view_rebuilds_on_queue_mutation() -> Result<()> {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);
    let baseline = app.filter_cache_stats();

    app.create_task_from_title("New Task", "2026-01-20T12:00:00Z")?;

    let after = app.filter_cache_stats();
    assert_eq!(after.id_index_rebuilds, baseline.id_index_rebuilds + 1);
    assert_eq!(after.filtered_rebuilds, baseline.filtered_rebuilds + 1);
    Ok(())
}

#[test]
fn rebuild_filtered_view_reuses_cache_for_normalized_tags() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task_with_tags("RQ-0001", "Task 1", vec!["alpha"])],
    };
    let mut app = App::new(queue);
    app.set_tag_filters(vec!["beta".to_string(), "alpha".to_string()]);
    let baseline = app.filter_cache_stats();

    app.set_tag_filters(vec!["alpha".to_string(), "beta".to_string()]);

    let after = app.filter_cache_stats();
    assert_eq!(after.filtered_rebuilds, baseline.filtered_rebuilds);
}

#[test]
fn rebuild_filtered_view_invalid_regex_is_cached() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);
    app.filters.search_options.use_regex = true;

    app.set_search_query("[".to_string());
    assert!(app.filtered_indices.is_empty());
    let baseline = app.filter_cache_stats();

    app.rebuild_filtered_view();

    let after = app.filter_cache_stats();
    assert_eq!(after.filtered_rebuilds, baseline.filtered_rebuilds);
    assert!(app.filtered_indices.is_empty());
    assert!(app
        .status_message
        .as_deref()
        .is_some_and(|message| message.contains("Search error")));
}

#[test]
fn rebuild_filtered_view_prefers_selected_task_when_present() {
    let t1 = make_test_task_with_tags("RQ-0001", "Task 1", vec!["tui"]);
    let t2 = make_test_task_with_tags("RQ-0002", "Task 2", vec!["tui"]);
    let queue = QueueFile {
        version: 1,
        tasks: vec![t1, t2],
    };
    let mut app = App::new(queue);
    app.filters.tags = vec!["tui".to_string()];
    app.rebuild_filtered_view();

    app.rebuild_filtered_view_with_preferred(Some("RQ-0002"));

    assert_eq!(app.selected, 1);
    assert_eq!(
        app.selected_task().map(|task| task.id.as_str()),
        Some("RQ-0002")
    );
}

#[test]
fn archive_terminal_tasks_noop_when_none_terminal() -> Result<()> {
    let queue = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
    };
    let mut app = App::new(queue);

    let now = "2026-01-20T12:00:00Z";
    let moved = app.archive_terminal_tasks(now)?;

    assert_eq!(moved, 0);
    assert_eq!(app.queue.tasks.len(), 1);
    assert_eq!(app.done.tasks.len(), 0);
    assert_eq!(
        app.status_message.as_deref(),
        Some("No done/rejected tasks to archive")
    );
    Ok(())
}

#[test]
fn archive_terminal_tasks_stamps_timestamps() -> Result<()> {
    let mut done_task = make_test_task("RQ-0001", "Task 1", TaskStatus::Done);
    done_task.updated_at = None;
    done_task.completed_at = None;

    let mut rejected_task = make_test_task("RQ-0002", "Task 2", TaskStatus::Rejected);
    rejected_task.updated_at = Some("2026-01-19T00:00:00Z".to_string());
    rejected_task.completed_at = Some("2026-01-19T00:00:00Z".to_string());

    let queue = QueueFile {
        version: 1,
        tasks: vec![done_task, rejected_task],
    };
    let mut app = App::new(queue);

    let now = "2026-01-20T12:00:00Z";
    let now_canon = canonical_rfc3339(now);
    let moved = app.archive_terminal_tasks(now)?;

    assert_eq!(moved, 2);
    assert!(app.dirty);
    assert!(app.dirty_done);
    assert_eq!(app.queue.tasks.len(), 0);
    assert_eq!(app.done.tasks.len(), 2);

    let done_archived = app
        .done
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("RQ-0001 archived");
    assert_eq!(
        done_archived.updated_at.as_deref(),
        Some(now_canon.as_str())
    );
    assert_eq!(
        done_archived.completed_at.as_deref(),
        Some(now_canon.as_str())
    );

    let rejected_archived = app
        .done
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("RQ-0002 archived");
    assert_eq!(
        rejected_archived.updated_at.as_deref(),
        Some(now_canon.as_str())
    );
    assert_eq!(
        rejected_archived.completed_at.as_deref(),
        Some("2026-01-19T00:00:00Z")
    );
    Ok(())
}

#[test]
fn palette_entries_include_scan_command() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("");
    assert!(entries
        .iter()
        .any(|entry| matches!(entry.cmd, PaletteCommand::ScanRepo)));
}

#[test]
fn scan_label_formats_focus() {
    assert_eq!(scan_label(""), "scan: (all)");
    assert_eq!(scan_label("  security "), "scan: security");
}

#[test]
fn start_scan_execution_sets_running_label() {
    let mut app = App::new(QueueFile::default());
    app.start_scan_execution("focus".to_string(), false, false);
    assert_eq!(app.running_task_id.as_deref(), Some("scan: focus"));
    assert!(matches!(app.running_kind, Some(RunningKind::Scan { .. })));
}

#[test]
fn filter_summary_includes_case_sensitive() {
    let mut app = App::new(QueueFile::default());
    app.filters.search_options.case_sensitive = true;
    app.filters.query = "test".to_string();

    let summary = app.filter_summary();
    assert!(summary.is_some());
    assert!(summary.as_ref().unwrap().contains("case-sensitive"));
    assert!(summary.as_ref().unwrap().contains("query=test"));
}

#[test]
fn filter_summary_includes_regex() {
    let mut app = App::new(QueueFile::default());
    app.filters.search_options.use_regex = true;
    app.filters.query = "RQ-\\d{4}".to_string();

    let summary = app.filter_summary();
    assert!(summary.is_some());
    assert!(summary.as_ref().unwrap().contains("regex"));
    assert!(summary.as_ref().unwrap().contains("query=RQ-\\d{4}"));
}

#[test]
fn filter_summary_includes_both_search_options() {
    let mut app = App::new(QueueFile::default());
    app.filters.search_options.use_regex = true;
    app.filters.search_options.case_sensitive = true;
    app.filters.query = "test".to_string();

    let summary = app.filter_summary();
    assert!(summary.is_some());
    assert!(summary.as_ref().unwrap().contains("regex"));
    assert!(summary.as_ref().unwrap().contains("case-sensitive"));
}

#[test]
fn has_active_filters_detects_search_options() {
    let mut app = App::new(QueueFile::default());

    assert!(!app.has_active_filters(), "no filters active by default");

    app.filters.search_options.use_regex = true;
    assert!(
        app.has_active_filters(),
        "regex option makes filters active"
    );

    app.filters.search_options.use_regex = false;
    assert!(!app.has_active_filters(), "regex option disabled");

    app.filters.search_options.case_sensitive = true;
    assert!(
        app.has_active_filters(),
        "case-sensitive option makes filters active"
    );

    app.filters.search_options.case_sensitive = false;
    assert!(!app.has_active_filters(), "case-sensitive option disabled");
}

#[test]
fn filter_summary_includes_scopes() {
    let mut app = App::new(QueueFile::default());
    app.filters.search_options.scopes = vec!["crates/ralph".to_string()];
    app.filters.query = "test".to_string();

    let summary = app.filter_summary();
    assert!(summary.is_some());
    assert!(summary.as_ref().unwrap().contains("scope=crates/ralph"));
    assert!(summary.as_ref().unwrap().contains("query=test"));
}

#[test]
fn has_active_filters_detects_scopes() {
    let mut app = App::new(QueueFile::default());

    assert!(!app.has_active_filters(), "no filters active by default");

    app.filters.search_options.scopes = vec!["frontend".to_string()];
    assert!(
        app.has_active_filters(),
        "scope filter makes filters active"
    );

    app.filters.search_options.scopes.clear();
    assert!(!app.has_active_filters(), "scope filter disabled");
}

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
