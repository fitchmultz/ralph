//! Tests for filtering, search, and cache behavior.
//!
//! Responsibilities:
//! - Validate tag filtering, search queries, and filter cache behavior.
//! - Test filter summaries and active filter detection.
//! - Exercise archive functionality and scan command integration.
//!
//! Not handled here:
//! - App state initialization, task editing, or persistence (see app_state.rs).
//! - Palette fuzzy matching or phase tracking (see other modules).

use super::super::app::*;
use super::super::app_execution::RunningKind;
use super::super::app_palette::scan_label;
use super::super::events::PaletteCommand;
use super::{canonical_rfc3339, make_test_task, make_test_task_with_tags, QueueFile, Result};
use crate::contracts::TaskStatus;

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
