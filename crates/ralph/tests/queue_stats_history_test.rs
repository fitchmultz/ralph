//! Integration tests for queue stats/history/burndown report output.

use anyhow::{Context, Result};
use ralph::timeutil;
use serde_json::Value;
use std::path::Path;
use std::process::ExitStatus;
use time::{Duration, OffsetDateTime};

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

fn write_empty_queue(dir: &Path) -> Result<()> {
    let queue = r#"{
  "version": 1,
  "tasks": []
}"#;
    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(dir.join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    std::fs::write(dir.join(".ralph/done.jsonc"), done).context("write done.json")?;
    Ok(())
}

fn format_date_key(dt: OffsetDateTime) -> String {
    format!("{:04}-{:02}-{:02}", dt.year(), dt.month() as u8, dt.day())
}

fn parse_json_output(stdout: &str) -> Result<Value> {
    serde_json::from_str(stdout).context("parse JSON output")
}

#[test]
fn stats_json_includes_summary_and_durations() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    let created_at = "2026-01-10T00:00:00Z";
    let done_at = "2026-01-10T02:00:00Z";
    let rejected_at = "2026-01-10T04:00:00Z";
    let queue = format!(
        r#"{{
  "version": 1,
  "tasks": [
    {{
      "id": "RQ-0100",
      "status": "done",
      "title": "Done task",
      "priority": "medium",
      "tags": ["Reports", "CLI"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "{created_at}",
      "completed_at": "{done_at}",
      "updated_at": "{done_at}"
    }},
    {{
      "id": "RQ-0101",
      "status": "rejected",
      "title": "Rejected task",
      "priority": "medium",
      "tags": ["reports"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "{created_at}",
      "completed_at": "{rejected_at}",
      "updated_at": "{rejected_at}"
    }}
  ]
}}"#
    );
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
    let summary = payload.get("summary").context("missing summary")?;
    anyhow::ensure!(summary.get("total").and_then(Value::as_u64) == Some(2));
    anyhow::ensure!(summary.get("done").and_then(Value::as_u64) == Some(1));
    anyhow::ensure!(summary.get("rejected").and_then(Value::as_u64) == Some(1));
    anyhow::ensure!(summary.get("terminal").and_then(Value::as_u64) == Some(2));
    anyhow::ensure!(summary.get("active").and_then(Value::as_u64) == Some(0));
    let terminal_rate = summary
        .get("terminal_rate")
        .and_then(Value::as_f64)
        .context("terminal_rate missing")?;
    anyhow::ensure!(
        (terminal_rate - 100.0).abs() < 0.01,
        "expected terminal rate 100.0, got {terminal_rate}"
    );

    let durations = payload.get("durations").context("missing durations")?;
    anyhow::ensure!(durations.get("count").and_then(Value::as_u64) == Some(2));
    anyhow::ensure!(
        durations.get("average_seconds").and_then(Value::as_i64) == Some(10800),
        "expected average_seconds 10800"
    );
    anyhow::ensure!(
        durations.get("median_seconds").and_then(Value::as_i64) == Some(14400),
        "expected median_seconds 14400"
    );
    anyhow::ensure!(
        durations.get("average_human").and_then(Value::as_str) == Some("3h"),
        "expected average_human 3h"
    );
    anyhow::ensure!(
        durations.get("median_human").and_then(Value::as_str) == Some("4h"),
        "expected median_human 4h"
    );

    let tags = payload
        .get("tag_breakdown")
        .and_then(Value::as_array)
        .context("missing tag_breakdown")?;
    anyhow::ensure!(tags.len() == 2, "expected 2 tag entries");
    anyhow::ensure!(tags[0].get("tag").and_then(Value::as_str) == Some("reports"));
    anyhow::ensure!(tags[0].get("count").and_then(Value::as_u64) == Some(2));
    anyhow::ensure!(tags[1].get("tag").and_then(Value::as_str) == Some("cli"));
    anyhow::ensure!(tags[1].get("count").and_then(Value::as_u64) == Some(1));

    Ok(())
}

#[test]
fn history_json_returns_window_and_days() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    let now = OffsetDateTime::now_utc();
    let day0 = timeutil::format_rfc3339(now).context("format day0")?;
    let day1 = timeutil::format_rfc3339(now - Duration::days(1)).context("format day1")?;
    let day2 = timeutil::format_rfc3339(now - Duration::days(2)).context("format day2")?;
    let queue = format!(
        r#"{{
  "version": 1,
  "tasks": [
    {{
      "id": "RQ-0200",
      "status": "todo",
      "title": "Created two days ago",
      "priority": "medium",
      "tags": ["reports"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "history",
      "created_at": "{day2}",
      "updated_at": "{day2}"
    }},
    {{
      "id": "RQ-0201",
      "status": "done",
      "title": "Created yesterday, completed today",
      "priority": "medium",
      "tags": ["reports"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "history",
      "created_at": "{day1}",
      "completed_at": "{day0}",
      "updated_at": "{day0}"
    }}
  ]
}}"#
    );
    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.jsonc"), done).context("write done.json")?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["queue", "history", "--days", "3", "--format", "json"],
    );
    anyhow::ensure!(
        status.success(),
        "expected history to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload = parse_json_output(&stdout)?;
    let window = payload.get("window").context("missing window")?;
    anyhow::ensure!(window.get("days").and_then(Value::as_i64) == Some(3));
    let start_date = window
        .get("start_date")
        .and_then(Value::as_str)
        .context("missing start_date")?;
    let end_date = window
        .get("end_date")
        .and_then(Value::as_str)
        .context("missing end_date")?;
    anyhow::ensure!(
        start_date == format_date_key(now - Duration::days(2)),
        "unexpected start_date {start_date}"
    );
    anyhow::ensure!(
        end_date == format_date_key(now),
        "unexpected end_date {end_date}"
    );

    let days = payload
        .get("days")
        .and_then(Value::as_array)
        .context("missing days")?;
    anyhow::ensure!(days.len() == 3, "expected 3 days");

    let day2_key = format_date_key(now - Duration::days(2));
    let day1_key = format_date_key(now - Duration::days(1));
    let day0_key = format_date_key(now);

    anyhow::ensure!(days[0].get("date").and_then(Value::as_str) == Some(day2_key.as_str()));
    anyhow::ensure!(
        days[0]
            .get("created")
            .and_then(Value::as_array)
            .map(|items| items.iter().any(|id| id.as_str() == Some("RQ-0200")))
            == Some(true)
    );

    anyhow::ensure!(days[1].get("date").and_then(Value::as_str) == Some(day1_key.as_str()));
    anyhow::ensure!(
        days[1]
            .get("created")
            .and_then(Value::as_array)
            .map(|items| items.iter().any(|id| id.as_str() == Some("RQ-0201")))
            == Some(true)
    );

    anyhow::ensure!(days[2].get("date").and_then(Value::as_str) == Some(day0_key.as_str()));
    anyhow::ensure!(
        days[2]
            .get("completed")
            .and_then(Value::as_array)
            .map(|items| items.iter().any(|id| id.as_str() == Some("RQ-0201")))
            == Some(true)
    );

    Ok(())
}

#[test]
fn burndown_json_empty_window_returns_zero_counts() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_empty_queue(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["queue", "burndown", "--days", "2", "--format", "json"],
    );
    anyhow::ensure!(
        status.success(),
        "expected burndown to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload = parse_json_output(&stdout)?;
    anyhow::ensure!(
        payload.get("max_count").and_then(Value::as_u64) == Some(0),
        "expected max_count 0"
    );
    let daily_counts = payload
        .get("daily_counts")
        .and_then(Value::as_array)
        .context("missing daily_counts")?;
    anyhow::ensure!(daily_counts.len() == 2, "expected two days");
    anyhow::ensure!(
        daily_counts
            .iter()
            .all(|day| day.get("remaining").and_then(Value::as_u64) == Some(0)),
        "expected all remaining counts to be 0"
    );

    Ok(())
}

#[test]
fn burndown_empty_window_reports_no_remaining_tasks() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_empty_queue(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "burndown", "--days", "2"]);
    anyhow::ensure!(
        status.success(),
        "expected burndown to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("No remaining tasks in the last 2 days."),
        "expected empty-window message\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        !stdout.contains("Remaining Tasks"),
        "expected no Remaining Tasks header when empty\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        !stdout.contains("█ ="),
        "expected no legend when empty\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn burndown_zero_count_day_renders_empty_bar() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    let now = OffsetDateTime::now_utc();
    let created_at = timeutil::format_rfc3339(now).context("format created_at")?;
    let queue = format!(
        r#"{{
  "version": 1,
  "tasks": [
    {{
      "id": "RQ-0001",
      "status": "todo",
      "title": "Open task",
      "priority": "medium",
      "tags": ["reports"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "burndown",
      "created_at": "{created_at}",
      "updated_at": "{created_at}"
    }}
  ]
}}"#
    );
    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.jsonc"), done).context("write done.json")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "burndown", "--days", "2"]);
    anyhow::ensure!(
        status.success(),
        "expected burndown to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let today_key = format_date_key(now);
    let yesterday_key = format_date_key(now - Duration::days(1));
    let today_prefix = format!("  {today_key} | ");
    let yesterday_prefix = format!("  {yesterday_key} | ");

    let yesterday_line = stdout
        .lines()
        .find(|line| line.starts_with(&yesterday_prefix))
        .context("find yesterday burndown line")?;
    anyhow::ensure!(
        !yesterday_line.contains('█'),
        "expected no bar for zero-count day\nline: {yesterday_line}\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        yesterday_line.trim_end().ends_with(" 0"),
        "expected yesterday count to be 0\nline: {yesterday_line}\nstdout:\n{stdout}"
    );

    let today_line = stdout
        .lines()
        .find(|line| line.starts_with(&today_prefix))
        .context("find today burndown line")?;
    anyhow::ensure!(
        today_line.contains('█'),
        "expected bar for non-zero day\nline: {today_line}\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        today_line.trim_end().ends_with(" 1"),
        "expected today count to be 1\nline: {today_line}\nstdout:\n{stdout}"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// Execution History ETA tests
// ------------------------------------------------------------------------

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

#[test]
fn stats_json_includes_velocity_breakdowns_by_tag_and_runner() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    let now = OffsetDateTime::now_utc();
    let recent_completed = timeutil::format_rfc3339(now).context("format recent")?;

    // Two tasks with different tags and runners, completed recently
    let queue = format!(
        r#"{{
  "version": 1,
  "tasks": [
    {{
      "id": "RQ-0001",
      "status": "done",
      "title": "Done task one",
      "priority": "medium",
      "tags": ["velocity-test", "shared-tag"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "{recent_completed}",
      "completed_at": "{recent_completed}",
      "updated_at": "{recent_completed}",
      "custom_fields": {{
        "runner_used": "codex"
      }}
    }},
    {{
      "id": "RQ-0002",
      "status": "done",
      "title": "Done task two",
      "priority": "medium",
      "tags": ["shared-tag"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "{recent_completed}",
      "completed_at": "{recent_completed}",
      "updated_at": "{recent_completed}",
      "custom_fields": {{
        "runner_used": "claude"
      }}
    }}
  ]
}}"#
    );
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
    let velocity = payload.get("velocity").context("missing velocity")?;

    // by_tag should include our tags
    let by_tag = velocity
        .get("by_tag")
        .and_then(Value::as_array)
        .context("missing velocity.by_tag")?;
    anyhow::ensure!(!by_tag.is_empty(), "expected velocity by_tag entries");

    // Find shared-tag which should have count 2 (both tasks)
    let shared_tag_entry = by_tag
        .iter()
        .find(|e| e.get("key").and_then(Value::as_str) == Some("shared-tag"))
        .context("shared-tag not found in velocity by_tag")?;
    anyhow::ensure!(
        shared_tag_entry.get("last_7_days").and_then(Value::as_u64) == Some(2),
        "expected shared-tag last_7_days = 2"
    );

    // by_runner should include codex and claude
    let by_runner = velocity
        .get("by_runner")
        .and_then(Value::as_array)
        .context("missing velocity.by_runner")?;
    anyhow::ensure!(!by_runner.is_empty(), "expected velocity by_runner entries");

    let runner_keys: Vec<&str> = by_runner
        .iter()
        .filter_map(|e| e.get("key").and_then(Value::as_str))
        .collect();
    anyhow::ensure!(
        runner_keys.contains(&"codex"),
        "expected codex in velocity by_runner"
    );
    anyhow::ensure!(
        runner_keys.contains(&"claude"),
        "expected claude in velocity by_runner"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// Slow Groups Tests
// ------------------------------------------------------------------------

#[test]
fn stats_json_includes_slow_groups_by_tag_and_runner() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    // Task with measurable work time (started -> completed)
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "done",
      "title": "Slow task",
      "priority": "medium",
      "tags": ["slow-tag"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "2026-01-10T00:00:00Z",
      "started_at": "2026-01-10T00:00:00Z",
      "completed_at": "2026-01-10T04:00:00Z",
      "updated_at": "2026-01-10T04:00:00Z",
      "custom_fields": {
        "runner_used": "codex"
      }
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
    let slow_groups = payload.get("slow_groups").context("missing slow_groups")?;

    // by_tag should include slow-tag with 4h median
    let by_tag = slow_groups
        .get("by_tag")
        .and_then(Value::as_array)
        .context("missing slow_groups.by_tag")?;
    anyhow::ensure!(!by_tag.is_empty(), "expected slow_groups by_tag entries");

    let slow_tag_entry = by_tag
        .iter()
        .find(|e| e.get("key").and_then(Value::as_str) == Some("slow-tag"))
        .context("slow-tag not found in slow_groups by_tag")?;
    anyhow::ensure!(
        slow_tag_entry.get("median_seconds").and_then(Value::as_i64) == Some(14400),
        "expected slow-tag median_seconds 14400 (4h)"
    );
    anyhow::ensure!(
        slow_tag_entry.get("count").and_then(Value::as_u64) == Some(1),
        "expected slow-tag count 1"
    );

    // by_runner should include codex with 4h median
    let by_runner = slow_groups
        .get("by_runner")
        .and_then(Value::as_array)
        .context("missing slow_groups.by_runner")?;
    anyhow::ensure!(
        !by_runner.is_empty(),
        "expected slow_groups by_runner entries"
    );

    let codex_entry = by_runner
        .iter()
        .find(|e| e.get("key").and_then(Value::as_str) == Some("codex"))
        .context("codex not found in slow_groups by_runner")?;
    anyhow::ensure!(
        codex_entry.get("median_seconds").and_then(Value::as_i64) == Some(14400),
        "expected codex median_seconds 14400 (4h)"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// Tag Filtering Tests
// ------------------------------------------------------------------------

#[test]
fn stats_tag_filtering_filters_results() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    // Two tasks with different tags
    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Task with tag A",
      "priority": "medium",
      "tags": ["tag-a"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "2026-01-10T00:00:00Z",
      "updated_at": "2026-01-10T00:00:00Z"
    },
    {
      "id": "RQ-0002",
      "status": "todo",
      "title": "Task with tag B",
      "priority": "medium",
      "tags": ["tag-b"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "2026-01-10T00:00:00Z",
      "updated_at": "2026-01-10T00:00:00Z"
    }
  ]
}"#;
    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.jsonc"), done).context("write done.json")?;

    // Filter by tag-a
    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["queue", "stats", "--tag", "tag-a", "--format", "json"],
    );
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload = parse_json_output(&stdout)?;
    let summary = payload.get("summary").context("missing summary")?;

    // Should only count 1 task (the one with tag-a)
    anyhow::ensure!(
        summary.get("total").and_then(Value::as_u64) == Some(1),
        "expected total 1 after filtering by tag-a"
    );

    // Filters should reflect the tag
    let filters = payload.get("filters").context("missing filters")?;
    let filter_tags = filters
        .get("tags")
        .and_then(Value::as_array)
        .context("missing filters.tags")?;
    anyhow::ensure!(
        filter_tags.len() == 1 && filter_tags[0].as_str() == Some("tag-a"),
        "expected filters.tags to contain tag-a"
    );

    // Tag breakdown should only show tag-a
    let tag_breakdown = payload
        .get("tag_breakdown")
        .and_then(Value::as_array)
        .context("missing tag_breakdown")?;
    anyhow::ensure!(
        tag_breakdown.len() == 1,
        "expected only 1 tag in breakdown after filtering"
    );
    anyhow::ensure!(
        tag_breakdown[0].get("tag").and_then(Value::as_str) == Some("tag-a"),
        "expected tag-a in breakdown"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// Text Output Section Tests
// ------------------------------------------------------------------------

#[test]
fn stats_text_shows_all_sections() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;

    // Use recent timestamps for velocity calculations (velocity requires completion in last 7 days)
    let now = OffsetDateTime::now_utc();
    let one_hour_ago = now - Duration::hours(1);
    let two_hours_ago = now - Duration::hours(2);
    let created_at = timeutil::format_rfc3339(two_hours_ago).context("format created_at")?;
    let started_at = timeutil::format_rfc3339(one_hour_ago).context("format started_at")?;
    let completed_at = timeutil::format_rfc3339(now).context("format completed_at")?;

    // Task with complete data for all sections
    let queue = format!(
        r#"{{
  "version": 1,
  "tasks": [
    {{
      "id": "RQ-0001",
      "status": "done",
      "title": "Complete task",
      "priority": "medium",
      "tags": ["text-section-test"],
      "scope": ["crates/ralph"],
      "evidence": ["test"],
      "plan": ["verify"],
      "request": "stats",
      "created_at": "{created_at}",
      "started_at": "{started_at}",
      "completed_at": "{completed_at}",
      "updated_at": "{completed_at}",
      "custom_fields": {{
        "runner_used": "codex"
      }}
    }}
  ]
}}"#
    );
    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(dir.path().join(".ralph/queue.jsonc"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.jsonc"), done).context("write done.json")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "stats"]);
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify all expected text sections are present
    anyhow::ensure!(
        stdout.contains("Task Statistics"),
        "expected 'Task Statistics' header\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Total tasks:"),
        "expected 'Total tasks:' line\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Terminal (done/rejected):"),
        "expected 'Terminal' line\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Lead Time"),
        "expected 'Lead Time' section\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Work Time"),
        "expected 'Work Time' section\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Start Lag"),
        "expected 'Start Lag' section\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Velocity by Tag"),
        "expected 'Velocity by Tag' section\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Slow Task Types by Tag"),
        "expected 'Slow Task Types by Tag' section\nstdout:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Tag Breakdown:"),
        "expected 'Tag Breakdown:' section\nstdout:\n{stdout}"
    );

    Ok(())
}

// ------------------------------------------------------------------------
// Empty Queue Tests
// ------------------------------------------------------------------------

#[test]
fn stats_empty_queue_shows_no_tasks_message() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    init_repo(dir.path())?;
    write_empty_queue(dir.path())?;

    // Text output
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "stats"]);
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("No tasks found."),
        "expected 'No tasks found.' for empty queue\nstdout:\n{stdout}"
    );

    // JSON output
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "stats", "--format", "json"]);
    anyhow::ensure!(
        status.success(),
        "expected stats to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload = parse_json_output(&stdout)?;
    let summary = payload.get("summary").context("missing summary")?;
    anyhow::ensure!(
        summary.get("total").and_then(Value::as_u64) == Some(0),
        "expected total 0 for empty queue"
    );
    anyhow::ensure!(
        summary.get("terminal_rate").and_then(Value::as_f64) == Some(0.0),
        "expected terminal_rate 0.0 for empty queue"
    );

    Ok(())
}
