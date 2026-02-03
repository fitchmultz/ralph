//! Integration tests for queue list/sort sort-by validation and ordering.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

mod test_support;

fn ralph_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ralph") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");
    let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        exe_dir
            .parent()
            .expect("deps directory should have a parent directory")
    } else {
        exe_dir
    };

    let bin_name = if cfg!(windows) { "ralph.exe" } else { "ralph" };
    let candidate = profile_dir.join(bin_name);
    if candidate.exists() {
        return candidate;
    }

    panic!(
        "CARGO_BIN_EXE_ralph was not set and fallback binary path does not exist: {}",
        candidate.display()
    );
}

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(ralph_bin())
        .current_dir(dir)
        .env_remove("RALPH_REPO_ROOT_OVERRIDE")
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn init_repo(dir: &Path) -> Result<()> {
    let (status, stdout, stderr) = run_in_dir(dir, &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Ok(())
}

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

    std::fs::write(dir.join(".ralph/queue.json"), queue).context("write queue.json")?;
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
    anyhow::ensure!(
        stderr.contains("nope") && stderr.contains("priority"),
        "expected clap error listing valid sort-by values, got:\n{stderr}"
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
        std::fs::read_to_string(dir.path().join(".ralph/queue.json")).context("read queue")?;
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
        std::fs::read_to_string(dir.path().join(".ralph/queue.json")).context("read queue")?;
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
        std::fs::read_to_string(dir.path().join(".ralph/queue.json")).context("read queue")?;
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
