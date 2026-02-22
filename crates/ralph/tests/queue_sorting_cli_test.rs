//! Integration tests for queue list/sort sort-by validation and ordering.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;
use std::process::ExitStatus;

mod test_support;

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
}
fn init_repo(dir: &Path) -> Result<()> {
    let (status, stdout, stderr) = run_in_dir(dir, &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Ok(())
}

/// Basic queue with priority variations for priority sort tests.
fn write_queue(dir: &Path) -> Result<()> {
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Low priority",
      "priority": "low",
      "tags": ["cli"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    },
    {
      "id": "RQ-0002",
      "status": "todo",
      "title": "Critical priority",
      "priority": "critical",
      "tags": ["cli"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    },
    {
      "id": "RQ-0003",
      "status": "todo",
      "title": "High priority",
      "priority": "high",
      "tags": ["cli"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;

    std::fs::write(dir.join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    Ok(())
}

/// Extended queue fixture with timestamps, statuses, and titles for comprehensive sort tests.
fn write_queue_for_sorting(dir: &Path) -> Result<()> {
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "draft",
      "title": "Zebra task",
      "priority": "low",
      "tags": ["test"],
      "scope": [],
      "created_at": "2026-01-10T00:00:00Z",
      "updated_at": "2026-01-15T00:00:00Z",
      "started_at": null,
      "scheduled_start": null
    },
    {
      "id": "RQ-0002",
      "status": "todo",
      "title": "Alpha task",
      "priority": "medium",
      "tags": ["test"],
      "scope": [],
      "created_at": "2026-01-15T00:00:00Z",
      "updated_at": "2026-01-20T00:00:00Z",
      "started_at": "2026-01-16T00:00:00Z",
      "scheduled_start": "2026-02-01T10:00:00Z"
    },
    {
      "id": "RQ-0003",
      "status": "doing",
      "title": "beta task",
      "priority": "high",
      "tags": ["test"],
      "scope": [],
      "created_at": "2026-01-20T00:00:00Z",
      "updated_at": "2026-01-25T00:00:00Z",
      "started_at": "2026-01-21T00:00:00Z",
      "scheduled_start": "2026-02-05T14:00:00Z"
    },
    {
      "id": "RQ-0004",
      "status": "done",
      "title": "GAMMA TASK",
      "priority": "critical",
      "tags": ["test"],
      "scope": [],
      "created_at": "2026-01-25T00:00:00Z",
      "updated_at": "2026-01-30T00:00:00Z",
      "completed_at": "2026-01-30T00:00:00Z",
      "started_at": "invalid-timestamp",
      "scheduled_start": "not-a-timestamp"
    },
    {
      "id": "RQ-0005",
      "status": "rejected",
      "title": "delta task",
      "priority": "low",
      "tags": ["test"],
      "scope": [],
      "created_at": "2026-01-12T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z",
      "completed_at": "2026-01-18T00:00:00Z",
      "started_at": "2026-01-13T00:00:00Z",
      "scheduled_start": "2026-02-03T09:00:00Z"
    }
  ]
}"#;

    std::fs::write(dir.join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    Ok(())
}

#[test]
fn queue_list_rejects_invalid_sort_by() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["queue", "list", "--sort-by", "nope"]);
    anyhow::ensure!(
        !status.success(),
        "expected non-zero exit for invalid sort-by"
    );
    // Verify that the error message contains several of the new valid values
    anyhow::ensure!(
        stderr.contains("nope"),
        "expected clap error to mention invalid value 'nope', got:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.contains("priority"),
        "expected clap error to list 'priority', got:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.contains("created_at"),
        "expected clap error to list 'created_at', got:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.contains("scheduled_start"),
        "expected clap error to list 'scheduled_start', got:\n{stderr}"
    );

    Ok(())
}

#[test]
fn queue_sort_rejects_invalid_sort_by() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["queue", "sort", "--sort-by", "nope"]);
    anyhow::ensure!(
        !status.success(),
        "expected non-zero exit for invalid sort-by"
    );
    anyhow::ensure!(
        stderr.contains("nope") && stderr.contains("priority"),
        "expected clap error listing valid sort-by values, got:\n{stderr}"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_priority_descending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "priority",
            "--order",
            "descending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();
    let expected = vec!["RQ-0002", "RQ-0003", "RQ-0001"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_defaults_to_descending_priority() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue(dir.path())?;

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["queue", "list", "--sort-by", "priority"]);
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();
    let expected = vec!["RQ-0002", "RQ-0003", "RQ-0001"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_priority_ascending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "priority",
            "--order",
            "ascending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();
    let expected = vec!["RQ-0001", "RQ-0003", "RQ-0002"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_sort_reorders_queue_by_priority_descending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "sort",
            "--sort-by",
            "priority",
            "--order",
            "descending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue sort failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue_str =
        std::fs::read_to_string(dir.path().join(".ralph/queue.jsonc")).context("read queue")?;
    let queue: Value = serde_json::from_str(&queue_str).context("parse queue json")?;
    let tasks = queue["tasks"]
        .as_array()
        .context("queue tasks should be array")?;
    let ids: Vec<&str> = tasks
        .iter()
        .filter_map(|task| task["id"].as_str())
        .collect();

    let expected = vec!["RQ-0002", "RQ-0003", "RQ-0001"];
    anyhow::ensure!(
        ids == expected,
        "unexpected queue order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_sort_reorders_queue_by_priority_ascending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "sort",
            "--sort-by",
            "priority",
            "--order",
            "ascending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue sort failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue_str =
        std::fs::read_to_string(dir.path().join(".ralph/queue.jsonc")).context("read queue")?;
    let queue: Value = serde_json::from_str(&queue_str).context("parse queue json")?;
    let tasks = queue["tasks"]
        .as_array()
        .context("queue tasks should be array")?;
    let ids: Vec<&str> = tasks
        .iter()
        .filter_map(|task| task["id"].as_str())
        .collect();

    let expected = vec!["RQ-0001", "RQ-0003", "RQ-0002"];
    anyhow::ensure!(
        ids == expected,
        "unexpected queue order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_sort_defaults_to_descending_priority() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue(dir.path())?;

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["queue", "sort", "--sort-by", "priority"]);
    anyhow::ensure!(
        status.success(),
        "queue sort failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue_str =
        std::fs::read_to_string(dir.path().join(".ralph/queue.jsonc")).context("read queue")?;
    let queue: Value = serde_json::from_str(&queue_str).context("parse queue json")?;
    let tasks = queue["tasks"]
        .as_array()
        .context("queue tasks should be array")?;
    let ids: Vec<&str> = tasks
        .iter()
        .filter_map(|task| task["id"].as_str())
        .collect();

    let expected = vec!["RQ-0002", "RQ-0003", "RQ-0001"];
    anyhow::ensure!(
        ids == expected,
        "unexpected queue order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// New sort-by field tests for queue list
// ------------------------------------------------------------------------

#[test]
fn queue_list_sorts_by_created_at_descending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "created_at",
            "--order",
            "descending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();
    // RQ-0004 (2026-01-25), RQ-0003 (2026-01-20), RQ-0002 (2026-01-15), RQ-0005 (2026-01-12), RQ-0001 (2026-01-10)
    let expected = vec!["RQ-0004", "RQ-0003", "RQ-0002", "RQ-0005", "RQ-0001"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_created_at_ascending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "created_at",
            "--order",
            "ascending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();
    // Ascending: oldest first - RQ-0001 (2026-01-10), RQ-0005 (2026-01-12), RQ-0002 (2026-01-15), RQ-0003 (2026-01-20), RQ-0004 (2026-01-25)
    let expected = vec!["RQ-0001", "RQ-0005", "RQ-0002", "RQ-0003", "RQ-0004"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_updated_at_descending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "updated_at",
            "--order",
            "descending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();
    // RQ-0004 (2026-01-30), RQ-0003 (2026-01-25), RQ-0002 (2026-01-20), RQ-0005 (2026-01-18), RQ-0001 (2026-01-15)
    let expected = vec!["RQ-0004", "RQ-0003", "RQ-0002", "RQ-0005", "RQ-0001"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_started_at_missing_last() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    // Test ascending: valid timestamps first (sorted ascending), then missing/invalid last
    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "started_at",
            "--order",
            "ascending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();

    // RQ-0005 (2026-01-13), RQ-0002 (2026-01-16), RQ-0003 (2026-01-21), then missing/invalid: RQ-0001 (null), RQ-0004 (invalid)
    // Missing/invalid sort last, and are tie-broken by id
    let expected = vec!["RQ-0005", "RQ-0002", "RQ-0003", "RQ-0001", "RQ-0004"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_started_at_descending_missing_last() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    // Test descending: valid timestamps first (sorted descending), then missing/invalid last
    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "started_at",
            "--order",
            "descending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();

    // Descending: RQ-0003 (2026-01-21), RQ-0002 (2026-01-16), RQ-0005 (2026-01-13), then missing/invalid: RQ-0001 (null), RQ-0004 (invalid)
    let expected = vec!["RQ-0003", "RQ-0002", "RQ-0005", "RQ-0001", "RQ-0004"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_scheduled_start_missing_last() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "scheduled_start",
            "--order",
            "ascending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();

    // Valid: RQ-0002 (2026-02-01), RQ-0005 (2026-02-03), RQ-0003 (2026-02-05)
    // Missing/invalid: RQ-0001 (null), RQ-0004 (invalid)
    let expected = vec!["RQ-0002", "RQ-0005", "RQ-0003", "RQ-0001", "RQ-0004"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_status_ascending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "status",
            "--order",
            "ascending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();

    // Ascending: draft < todo < doing < done < rejected
    // RQ-0001 (draft), RQ-0002 (todo), RQ-0003 (doing), RQ-0004 (done), RQ-0005 (rejected)
    let expected = vec!["RQ-0001", "RQ-0002", "RQ-0003", "RQ-0004", "RQ-0005"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_status_descending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "status",
            "--order",
            "descending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();

    // Descending: rejected > done > doing > todo > draft
    // RQ-0005 (rejected), RQ-0004 (done), RQ-0003 (doing), RQ-0002 (todo), RQ-0001 (draft)
    let expected = vec!["RQ-0005", "RQ-0004", "RQ-0003", "RQ-0002", "RQ-0001"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

#[test]
fn queue_list_sorts_by_title_case_insensitive_ascending() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue_for_sorting(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "queue",
            "list",
            "--sort-by",
            "title",
            "--order",
            "ascending",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "queue list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let ids: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('\t').next())
        .collect();

    // Case-insensitive ascending: Alpha, beta, delta, GAMMA, Zebra
    // RQ-0002 (Alpha), RQ-0003 (beta), RQ-0005 (delta), RQ-0004 (GAMMA), RQ-0001 (Zebra)
    let expected = vec!["RQ-0002", "RQ-0003", "RQ-0005", "RQ-0004", "RQ-0001"];
    anyhow::ensure!(
        ids == expected,
        "unexpected sort order: {ids:?} (expected {expected:?})"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// Dry-run tests for queue sort
// ------------------------------------------------------------------------

#[test]
fn queue_sort_dry_run_does_not_modify_file() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue(dir.path())?;

    let before_queue = std::fs::read_to_string(dir.path().join(".ralph/queue.jsonc"))?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "sort", "--dry-run"]);
    anyhow::ensure!(
        status.success(),
        "queue sort --dry-run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify dry-run message appears
    anyhow::ensure!(
        stderr.contains("Dry run") || stdout.contains("Dry run"),
        "expected dry-run message, got stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify file unchanged
    let after_queue = std::fs::read_to_string(dir.path().join(".ralph/queue.jsonc"))?;
    anyhow::ensure!(
        before_queue == after_queue,
        "queue.json changed during dry-run"
    );

    Ok(())
}

#[test]
fn queue_sort_dry_run_shows_new_order() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_queue(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "sort", "--dry-run"]);
    anyhow::ensure!(
        status.success(),
        "queue sort --dry-run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let output = format!("{stdout}\n{stderr}");
    // The dry-run should show the new order (critical, high, low priority)
    // RQ-0002 (critical), RQ-0003 (high), RQ-0001 (low)
    anyhow::ensure!(
        output.contains("RQ-0002") && output.contains("RQ-0003") && output.contains("RQ-0001"),
        "expected task IDs in new order, got:\n{output}"
    );

    Ok(())
}

#[test]
fn queue_sort_dry_run_already_sorted() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    // Create a queue that is already sorted by priority (descending)
    let queue = r#"{
      "version": 1,
      "tasks": [
        {
          "id": "RQ-0001",
          "status": "todo",
          "title": "Critical priority",
          "priority": "critical",
          "tags": ["cli"],
          "scope": ["crates/ralph"],
          "evidence": ["test"],
          "plan": ["verify"],
          "request": "test",
          "created_at": "2026-01-18T00:00:00Z",
          "updated_at": "2026-01-18T00:00:00Z"
        },
        {
          "id": "RQ-0002",
          "status": "todo",
          "title": "Low priority",
          "priority": "low",
          "tags": ["cli"],
          "scope": ["crates/ralph"],
          "evidence": ["test"],
          "plan": ["verify"],
          "request": "test",
          "created_at": "2026-01-18T00:00:00Z",
          "updated_at": "2026-01-18T00:00:00Z"
        }
      ]
    }"#;

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue).context("write queue.json")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "sort", "--dry-run"]);
    anyhow::ensure!(
        status.success(),
        "queue sort --dry-run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let output = format!("{stdout}\n{stderr}");
    anyhow::ensure!(
        output.contains("no changes") || output.contains("already sorted"),
        "expected 'already sorted' or 'no changes' message, got:\n{output}"
    );

    Ok(())
}
