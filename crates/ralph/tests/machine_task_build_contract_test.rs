//! Purpose: machine task-build contract regression tests.
//!
//! Responsibilities:
//! - Verify `ralph machine task build` emits a single clean JSON document.
//! - Guard the app-facing task-build path against noisy runner stdout/stderr.
//!
//! Scope:
//! - End-to-end CLI contract coverage using a local fake runner.
//! - Machine task-build output hygiene only.
//!
//! Usage:
//! - Run through Cargo integration tests.
//!
//! Invariants/assumptions callers must respect:
//! - Machine commands reserve stdout for versioned JSON contracts.
//! - Human runner streams must not be parsed by app workflows or mixed into machine stdout.

use anyhow::{Context, Result};
use ralph::contracts::{MACHINE_TASK_BUILD_VERSION, MachineTaskBuildDocument};

mod test_support;

#[test]
fn machine_task_build_suppresses_noisy_runner_output_before_json() -> Result<()> {
    let dir = tempfile::tempdir().context("create temp repo")?;
    test_support::ralph_init(dir.path()).context("init ralph repo")?;

    let runner_path = test_support::create_fake_runner(
        dir.path(),
        "codex",
        r#"#!/bin/sh
cat >/dev/null
echo "NOISY_RUNNER_STDOUT_BEFORE_MACHINE_JSON"
echo "NOISY_RUNNER_STDERR_BEFORE_MACHINE_JSON" >&2
cat <<'JSON' > .ralph/queue.jsonc
{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Built by noisy runner",
      "priority": "medium",
      "tags": ["machine"],
      "scope": ["crates/ralph"],
      "evidence": [],
      "plan": [],
      "notes": [],
      "request": "Build a task without leaking runner output",
      "created_at": "2026-04-23T00:00:00Z",
      "updated_at": "2026-04-23T00:00:00Z"
    }
  ]
}
JSON
echo '{"type":"item.completed","item":{"type":"agent_message","text":"created task"}}'
"#,
    )?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.3-codex", Some(&runner_path))?;

    let request_path = dir.path().join("machine-task-build-request.json");
    std::fs::write(
        &request_path,
        serde_json::json!({
            "version": MACHINE_TASK_BUILD_VERSION,
            "request": "Build a task without leaking runner output",
            "tags": ["machine"],
            "scope": ["crates/ralph"]
        })
        .to_string(),
    )
    .context("write machine task build request")?;

    let request_arg = request_path.to_string_lossy().to_string();
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["machine", "task", "build", "--input", &request_arg],
    );

    assert!(
        status.success(),
        "machine task build should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("NOISY_RUNNER_STDOUT_BEFORE_MACHINE_JSON"),
        "runner stdout leaked into machine stdout:\n{stdout}"
    );
    assert!(
        !stderr.contains("NOISY_RUNNER_STDERR_BEFORE_MACHINE_JSON"),
        "runner stderr leaked into machine stderr:\n{stderr}"
    );

    let document: MachineTaskBuildDocument =
        serde_json::from_str(&stdout).context("parse machine task build stdout as JSON")?;
    assert_eq!(document.version, MACHINE_TASK_BUILD_VERSION);
    assert_eq!(document.result.created_count, 1);
    assert_eq!(document.result.task_ids, vec!["RQ-0001".to_string()]);
    assert_eq!(document.result.tasks[0].title, "Built by noisy runner");

    Ok(())
}
