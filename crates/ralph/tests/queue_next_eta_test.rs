//! Integration tests for `ralph queue next --with-eta`.
//!
//! Purpose:
//! - Integration tests for `ralph queue next --with-eta`.
//!
//! Responsibilities:
//! - Validate `--with-eta` output for both runnable tasks and the "no runnable task" case.
//! - Validate column behavior when combined with `--with-title`.
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
//! - ETA is appended as a final tab-separated column.

use anyhow::Result;
use std::path::Path;

mod test_support;

fn run_in_dir(dir: &Path, args: &[&str]) -> (std::process::ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
}

fn init_repo(dir: &Path) -> Result<()> {
    test_support::ralph_init(dir)?;
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

#[test]
fn queue_next_without_eta_prints_id_only() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_with_todo(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "next"]);
    assert!(status.success(), "expected success\nstderr:\n{stderr}");
    assert_eq!(stdout.trim(), "RQ-0001", "expected only task ID");
    Ok(())
}

#[test]
fn queue_next_with_eta_no_history_shows_na() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_with_todo(dir.path())?;
    // No execution history written

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "next", "--with-eta"]);
    assert!(status.success(), "expected success\nstderr:\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "RQ-0001\tn/a",
        "expected ID and n/a for missing history"
    );
    Ok(())
}

#[test]
fn queue_next_with_eta_with_history_shows_estimate() -> Result<()> {
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

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "next", "--with-eta"]);
    assert!(status.success(), "expected success\nstderr:\n{stderr}");
    // Should print ID and ETA (3m 30s = 210s)
    let output = stdout.trim();
    assert!(
        output.starts_with("RQ-0001\t"),
        "expected ID followed by tab"
    );
    assert!(
        output.contains("3m 30s") || output.contains("210s"),
        "expected ETA formatted duration, got: {output}"
    );
    Ok(())
}

#[test]
fn queue_next_with_eta_and_title() -> Result<()> {
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

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["queue", "next", "--with-title", "--with-eta"]);
    assert!(status.success(), "expected success\nstderr:\n{stderr}");
    let output = stdout.trim();
    // Output format: ID\tTITLE\tETA
    assert!(
        output.starts_with("RQ-0001\t"),
        "expected ID followed by tab"
    );
    assert!(output.contains("Test task"), "expected title in output");
    assert!(
        output.contains("3m 30s") || output.contains("210s"),
        "expected ETA formatted duration, got: {output}"
    );
    Ok(())
}

#[test]
fn queue_next_with_eta_no_runnable_task() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    // Empty queue - no runnable task
    let queue = r#"{"version": 1, "tasks": []}"#;
    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue)?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "next", "--with-eta"]);
    assert!(
        status.success(),
        "expected success even with no runnable task\nstderr:\n{stderr}"
    );
    // Should print next available ID with n/a
    let output = stdout.trim();
    assert!(
        output.contains("\tn/a") || output.starts_with("RQ-"),
        "expected next ID with n/a, got: {output}"
    );
    Ok(())
}
