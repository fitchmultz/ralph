//! Integration tests for `ralph queue list --with-eta`.
//!
//! Purpose:
//! - Integration tests for `ralph queue list --with-eta`.
//!
//! Responsibilities:
//! - Validate the `--with-eta` column behavior in `--format compact` and `--format long`.
//! - Assert that `--format json` output shape is unchanged (the flag is intentionally ignored).
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled:
//! - Deep validation of ETA math (unit tests cover calculation details).
//! - Full runner/model resolution matrix (these tests focus on the config-default path).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Seeded execution history produces a stable human ETA (e.g., `3m 30s` for 210s).
//! - Missing execution history yields `n/a`.
//! - ETA appears as the final tab-separated column for text formats.

use anyhow::Result;
use serde_json::Value;
use std::path::Path;

mod test_support;

fn run_in_dir(dir: &Path, args: &[&str]) -> (std::process::ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
}

fn init_repo(dir: &Path) -> Result<()> {
    test_support::seed_ralph_dir(dir)?;
    Ok(())
}

fn write_queue_with_todo(dir: &Path) -> Result<()> {
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Test task",
      "tags": ["rust"],
      "scope": ["crates/ralph"],
      "evidence": ["integration test fixture"],
      "plan": ["run preflight"],
      "request": "integration test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
    std::fs::write(dir.join(".ralph/queue.jsonc"), queue)?;
    Ok(())
}

fn write_queue_with_mixed_status(dir: &Path) -> Result<()> {
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Todo task",
      "tags": ["rust"],
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    },
    {
      "id": "RQ-0002",
      "status": "doing",
      "title": "Doing task",
      "tags": ["rust"],
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    },
    {
      "id": "RQ-0003",
      "status": "done",
      "title": "Done task",
      "tags": ["rust"],
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z",
      "completed_at": "2026-01-19T00:00:00Z"
    }
  ]
}"#;
    std::fs::write(dir.join(".ralph/queue.jsonc"), queue)?;
    Ok(())
}

#[test]
fn queue_list_compact_without_eta() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_with_todo(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "list"]);
    assert!(status.success(), "expected success\nstderr:\n{stderr}");
    // Compact format: ID\tSTATUS\tPRIORITY\tTITLE
    let output = stdout.trim();
    assert!(output.contains("RQ-0001"), "expected task ID");
    assert!(output.contains("todo"), "expected status");
    assert!(output.contains("Test task"), "expected title");
    // Should NOT have 5 tab-separated columns (ID, STATUS, PRIORITY, TITLE, ETA)
    let tab_count = output.matches('\t').count();
    assert!(
        tab_count < 4,
        "expected no ETA column, got {} tabs",
        tab_count
    );
    Ok(())
}

#[test]
fn queue_list_compact_with_eta_no_history() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_with_todo(dir.path())?;
    // No execution history

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "list", "--with-eta"]);
    assert!(status.success(), "expected success\nstderr:\n{stderr}");
    let output = stdout.trim();
    // Compact format with ETA: ID\tSTATUS\tPRIORITY\tTITLE\tETA
    assert!(output.contains("RQ-0001"), "expected task ID");
    assert!(output.contains("\tn/a"), "expected n/a for missing history");
    Ok(())
}

#[test]
fn queue_list_compact_with_eta_with_history() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    test_support::configure_agent_runner_model_phases(dir.path(), "codex", "gpt-5.3", 3)?;
    write_queue_with_todo(dir.path())?;
    test_support::write_execution_history_v1_single_sample(
        dir.path(),
        "codex",
        "gpt-5.3",
        210,
        60,
        120,
        30,
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "list", "--with-eta"]);
    assert!(status.success(), "expected success\nstderr:\n{stderr}");
    let output = stdout.trim();
    // Should have ETA column with duration
    assert!(output.contains("RQ-0001"), "expected task ID");
    assert!(
        output.contains("3m 30s") || output.contains("210s"),
        "expected ETA formatted duration, got: {output}"
    );
    Ok(())
}

#[test]
fn queue_list_long_with_eta() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    test_support::configure_agent_runner_model_phases(dir.path(), "codex", "gpt-5.3", 3)?;
    write_queue_with_todo(dir.path())?;
    test_support::write_execution_history_v1_single_sample(
        dir.path(),
        "codex",
        "gpt-5.3",
        210,
        60,
        120,
        30,
    )?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["queue", "list", "--format", "long", "--with-eta"],
    );
    assert!(status.success(), "expected success\nstderr:\n{stderr}");
    let output = stdout.trim();
    // Long format should also have ETA appended
    assert!(output.contains("RQ-0001"), "expected task ID");
    assert!(
        output.contains("3m 30s") || output.contains("210s"),
        "expected ETA formatted duration, got: {output}"
    );
    Ok(())
}

#[test]
fn queue_list_json_ignores_with_eta() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_with_todo(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["queue", "list", "--format", "json", "--with-eta"],
    );
    assert!(status.success(), "expected success\nstderr:\n{stderr}");

    // JSON should parse successfully and NOT contain ETA field
    let parsed: Value = serde_json::from_str(&stdout)?;
    let tasks = parsed.as_array().expect("expected array");
    assert_eq!(tasks.len(), 1);

    let task = &tasks[0];
    assert_eq!(task["id"], "RQ-0001");
    // JSON output should NOT have an eta field (it's opt-in for text formats only)
    assert!(
        task.get("eta").is_none(),
        "JSON should not include ETA field"
    );
    Ok(())
}

#[test]
fn queue_list_with_eta_mixed_status() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    test_support::configure_agent_runner_model_phases(dir.path(), "codex", "gpt-5.3", 3)?;
    write_queue_with_mixed_status(dir.path())?;
    test_support::write_execution_history_v1_single_sample(
        dir.path(),
        "codex",
        "gpt-5.3",
        210,
        60,
        120,
        30,
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "list", "--with-eta"]);
    assert!(status.success(), "expected success\nstderr:\n{stderr}");

    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "expected 3 tasks");

    // Todo and Doing should have ETA; Done should show n/a
    let todo_line = lines.iter().find(|l| l.contains("RQ-0001")).unwrap();
    let doing_line = lines.iter().find(|l| l.contains("RQ-0002")).unwrap();
    let done_line = lines.iter().find(|l| l.contains("RQ-0003")).unwrap();

    // Todo task should have ETA from history
    assert!(
        todo_line.contains("3m 30s") || todo_line.contains("210s"),
        "todo task should have ETA, got: {todo_line}"
    );

    // Doing task should have n/a (in-progress tasks don't have ETA in CLI)
    assert!(
        doing_line.contains("\tn/a"),
        "doing task should show n/a, got: {doing_line}"
    );

    // Done task should have n/a (terminal tasks don't need ETA)
    assert!(
        done_line.contains("\tn/a"),
        "done task should show n/a, got: {done_line}"
    );

    Ok(())
}
