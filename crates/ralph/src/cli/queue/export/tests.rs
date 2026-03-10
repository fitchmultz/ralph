//! Focused tests for queue export filtering and rendering helpers.
//!
//! Responsibilities:
//! - Cover export renderer output contracts and date parsing behavior.
//! - Keep formatter regression tests out of production modules.
//! - Provide stable fixtures for CSV, JSON, Markdown, and GitHub export paths.
//!
//! Not handled here:
//! - Command-level clap/help tests (see `cli/queue/tests/export.rs`).
//! - Broader queue command integration tests.
//!
//! Invariants/assumptions:
//! - Test tasks remain deterministic and self-contained.
//! - Renderer output expectations are intentionally high-signal rather than exhaustive.

use std::collections::HashMap;

use crate::contracts::{Task, TaskPriority, TaskStatus};

use super::filter::parse_date_filter;
use super::render::{render_export, render_task_as_github_issue_body};

fn create_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        title: title.to_string(),
        description: None,
        status,
        priority: TaskPriority::Medium,
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["evidence".to_string()],
        plan: vec!["step 1".to_string(), "step 2".to_string()],
        notes: vec!["note".to_string()],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-15T00:00:00Z".to_string()),
        updated_at: Some("2026-01-15T12:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        estimated_minutes: None,
        actual_minutes: None,
        depends_on: vec!["RQ-0001".to_string()],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    }
}

#[test]
fn csv_export_includes_all_fields() {
    let task = create_test_task("RQ-0002", "Test Task", TaskStatus::Todo);
    let csv = render_export(crate::cli::queue::QueueExportFormat::Csv, &[&task]).unwrap();

    assert!(csv.contains("id,title,status,priority"));
    assert!(csv.contains("parent_id"));
    assert!(csv.contains("RQ-0002"));
    assert!(csv.contains("Test Task"));
    assert!(csv.contains("todo"));
    assert!(csv.contains("medium"));
    assert!(csv.contains("test"));
    assert!(csv.contains("crates/ralph"));
}

#[test]
fn tsv_export_uses_tab_delimiter() {
    let task = create_test_task("RQ-0001", "Test", TaskStatus::Done);
    let tsv = render_export(crate::cli::queue::QueueExportFormat::Tsv, &[&task]).unwrap();

    assert!(tsv.contains("id\ttitle\tstatus"));
    assert!(!tsv.lines().nth(1).expect("row").contains(','));
}

#[test]
fn json_export_produces_valid_json() {
    let task = create_test_task("RQ-0001", "Test Task", TaskStatus::Todo);
    let json = render_export(crate::cli::queue::QueueExportFormat::Json, &[&task]).unwrap();

    assert!(json.starts_with('['));
    assert!(json.ends_with(']'));
    assert!(json.contains("RQ-0001"));
    assert!(json.contains("Test Task"));
}

#[test]
fn parse_date_filter_accepts_rfc3339() {
    assert!(parse_date_filter("2026-01-15T00:00:00Z").unwrap() > 0);
}

#[test]
fn parse_date_filter_accepts_ymd() {
    assert!(parse_date_filter("2026-01-15").unwrap() > 0);
}

#[test]
fn parse_date_filter_rejects_invalid() {
    assert!(parse_date_filter("not-a-date").is_err());
}

#[test]
fn markdown_export_produces_valid_table() {
    let task1 = create_test_task("RQ-0001", "First Task", TaskStatus::Todo);
    let task2 = create_test_task("RQ-0002", "Second Task", TaskStatus::Doing);
    let markdown =
        render_export(crate::cli::queue::QueueExportFormat::Md, &[&task1, &task2]).unwrap();

    assert!(markdown.contains("| ID | Status | Priority | Title |"));
    assert!(markdown.contains("|---|---|---"));
    assert!(markdown.contains("RQ-0001"));
    assert!(markdown.contains("First Task"));
    assert!(markdown.contains("todo"));
    assert!(markdown.contains("RQ-0002"));
}

#[test]
fn markdown_export_escapes_pipes() {
    let task = create_test_task("RQ-0001", "Task | With | Pipes", TaskStatus::Todo);
    let markdown = render_export(crate::cli::queue::QueueExportFormat::Md, &[&task]).unwrap();

    assert!(markdown.contains("Task \\| With \\| Pipes"));
}

#[test]
fn markdown_export_is_deterministic() {
    let task1 = create_test_task("RQ-0002", "Second", TaskStatus::Todo);
    let task2 = create_test_task("RQ-0001", "First", TaskStatus::Todo);

    let first = render_export(crate::cli::queue::QueueExportFormat::Md, &[&task1, &task2]).unwrap();
    let second =
        render_export(crate::cli::queue::QueueExportFormat::Md, &[&task1, &task2]).unwrap();

    assert_eq!(first, second);
    assert!(first.find("RQ-0001").unwrap() < first.find("RQ-0002").unwrap());
}

#[test]
fn github_issue_body_omits_empty_list_sections() {
    let mut task = create_test_task("RQ-0001", "Test", TaskStatus::Todo);
    task.plan.clear();
    task.evidence = vec!["Some evidence".to_string()];

    let body = render_task_as_github_issue_body(&task);
    assert!(body.contains("### Evidence"));
    assert!(!body.contains("### Plan\n\n"));
}

#[test]
fn github_export_produces_valid_markdown() {
    let task = create_test_task("RQ-0001", "Test Task", TaskStatus::Todo);
    let github = render_export(crate::cli::queue::QueueExportFormat::Gh, &[&task]).unwrap();

    assert!(github.contains("## RQ-0001: Test Task"));
    assert!(github.contains("**Status:**"));
    assert!(github.contains("**Priority:**"));
    assert!(github.contains("### Plan"));
    assert!(github.contains("### Evidence"));
}

#[test]
fn github_export_multiple_tasks_separates_with_hr() {
    let task1 = create_test_task("RQ-0001", "First", TaskStatus::Todo);
    let task2 = create_test_task("RQ-0002", "Second", TaskStatus::Todo);
    let github =
        render_export(crate::cli::queue::QueueExportFormat::Gh, &[&task1, &task2]).unwrap();

    assert!(github.contains("\n---\n"));
}
