//! ETA and time-tracking stats tests.
//!
//! Purpose:
//! - ETA and time-tracking stats tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;

#[test]
fn stats_json_includes_execution_history_eta_when_present() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    test_support::configure_agent_runner_model_phases(dir.path(), "codex", "gpt-5.3", 3)?;

    // Create a simple queue
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Test task",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue)?;

    // Write execution history
    test_support::write_execution_history_v1_single_sample(
        dir.path(),
        "codex",
        "gpt-5.3",
        210,
        60,
        120,
        30,
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "stats", "--format", "json"]);
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload = parse_json_output(&stdout)?;

    // Check execution_history_eta field exists and has expected structure
    let eta = payload
        .get("execution_history_eta")
        .context("missing execution_history_eta")?;

    anyhow::ensure!(
        eta.get("runner").and_then(Value::as_str) == Some("codex"),
        "expected runner to be codex"
    );
    anyhow::ensure!(
        eta.get("model").and_then(Value::as_str) == Some("gpt-5.3"),
        "expected model to be gpt-5.3"
    );
    anyhow::ensure!(
        eta.get("phase_count").and_then(Value::as_u64) == Some(3),
        "expected phase_count to be 3"
    );
    anyhow::ensure!(
        eta.get("sample_count").and_then(Value::as_u64) == Some(1),
        "expected sample_count to be 1"
    );
    anyhow::ensure!(
        eta.get("estimated_total_seconds").and_then(Value::as_u64) == Some(210),
        "expected estimated_total_seconds to be 210"
    );
    anyhow::ensure!(
        eta.get("estimated_total_human").and_then(Value::as_str) == Some("3m 30s"),
        "expected estimated_total_human to be 3m 30s"
    );
    anyhow::ensure!(
        eta.get("confidence").and_then(Value::as_str) == Some("low"),
        "expected confidence to be low (only 1 sample)"
    );

    Ok(())
}

#[test]
fn stats_json_execution_history_eta_null_when_no_history() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    // Create a simple queue without execution history
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Test task",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue)?;
    // No execution history written

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "stats", "--format", "json"]);
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload = parse_json_output(&stdout)?;

    // execution_history_eta should be null when no history
    anyhow::ensure!(
        payload.get("execution_history_eta").is_some(),
        "execution_history_eta field should exist even when null"
    );
    anyhow::ensure!(
        payload.get("execution_history_eta").unwrap().is_null(),
        "execution_history_eta should be null when no history"
    );

    Ok(())
}

#[test]
fn stats_text_includes_execution_history_eta_section() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    test_support::configure_agent_runner_model_phases(dir.path(), "codex", "gpt-5.3", 3)?;

    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Test task",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue)?;
    test_support::write_execution_history_v1_single_sample(
        dir.path(),
        "codex",
        "gpt-5.3",
        210,
        60,
        120,
        30,
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "stats"]);
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Text output should include the ETA section
    anyhow::ensure!(
        stdout.contains("Execution History ETA"),
        "expected 'Execution History ETA' section in text output\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("runner=codex"),
        "expected runner info in ETA section\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Samples: 1"),
        "expected sample count in ETA section\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Estimated new task:"),
        "expected estimated duration in ETA section\nstdout:\n{stdout}"
    );

    Ok(())
}

#[test]
fn stats_text_shows_na_when_no_history() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Test task",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue)?;
    // No execution history

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "stats"]);
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Text output should show n/a for missing history
    anyhow::ensure!(
        stdout.contains("Execution History ETA: n/a"),
        "expected 'Execution History ETA: n/a' in text output\nstdout:\n{stdout}"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// Time Tracking Tests (work_time and start_lag)
// ------------------------------------------------------------------------

#[test]
fn stats_json_includes_time_tracking_work_time_and_start_lag() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    // Tasks with started_at and completed_at for work_time calculation
    // work_time = started_at -> completed_at
    // start_lag = created_at -> started_at
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "done",
      "title": "Done task with work time",
      "priority": "medium",
      "tags": ["testing"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "2026-01-10T00:00:00Z",
      "started_at": "2026-01-10T01:00:00Z",
      "completed_at": "2026-01-10T03:00:00Z",
      "updated_at": "2026-01-10T03:00:00Z"
    }
  ]
}"#;
    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.jsonc"), done).context("write done.json")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "stats", "--format", "json"]);
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload = parse_json_output(&stdout)?;
    let time_tracking = payload
        .get("time_tracking")
        .context("missing time_tracking")?;

    // work_time: 1h -> 3h = 2h = 7200 seconds
    let work_time = time_tracking
        .get("work_time")
        .context("missing work_time")?;
    anyhow::ensure!(work_time.get("count").and_then(Value::as_u64) == Some(1));
    anyhow::ensure!(
        work_time.get("average_seconds").and_then(Value::as_i64) == Some(7200),
        "expected average_seconds 7200 (2h)"
    );

    // start_lag: 0h -> 1h = 1h = 3600 seconds
    let start_lag = time_tracking
        .get("start_lag")
        .context("missing start_lag")?;
    anyhow::ensure!(start_lag.get("count").and_then(Value::as_u64) == Some(1));
    anyhow::ensure!(
        start_lag.get("average_seconds").and_then(Value::as_i64) == Some(3600),
        "expected average_seconds 3600 (1h)"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// Velocity Breakdown Tests
// ------------------------------------------------------------------------
