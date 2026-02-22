//! Integration tests for `ralph queue next` behavior.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::ExitStatus;

mod test_support;

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
}

#[test]
fn queue_next_reports_empty_queue_with_done_tasks() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue = r#"{
  "version": 1,
  "tasks": []
}"#;

    let done = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "done",
      "title": "Finished task",
      "tags": ["rust"],
      "scope": ["crates/ralph"],
      "evidence": ["integration test"],
      "plan": ["noop"],
      "request": "done",
      "created_at": "2026-01-18T00:00:00.000000000Z",
      "updated_at": "2026-01-18T00:00:00.000000000Z",
      "completed_at": "2026-01-18T00:00:00.000000000Z"
    }
  ]
}"#;

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.jsonc"), done).context("write done.json")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "next"]);
    anyhow::ensure!(
        status.success(),
        "expected queue next to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.trim() == "RQ-0002",
        "expected next id when queue is empty\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}
