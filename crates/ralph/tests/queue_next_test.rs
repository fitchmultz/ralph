//! Integration tests for `ralph queue next` behavior.

use anyhow::{Context, Result};
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

    std::fs::write(dir.path().join(".ralph/queue.json"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.json"), done).context("write done.json")?;

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
