//! Integration tests for queue stats/history/burndown report output.

use anyhow::{Context, Result};
use ralph::timeutil;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use time::{Duration, OffsetDateTime};

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

fn write_empty_queue(dir: &Path) -> Result<()> {
    let queue = r#"{
  "version": 1,
  "tasks": []
}"#;
    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(dir.join(".ralph/queue.json"), queue).context("write queue.json")?;
    std::fs::write(dir.join(".ralph/done.json"), done).context("write done.json")?;
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

    std::fs::write(dir.path().join(".ralph/queue.json"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.json"), done).context("write done.json")?;

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

    std::fs::write(dir.path().join(".ralph/queue.json"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.json"), done).context("write done.json")?;

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

    std::fs::write(dir.path().join(".ralph/queue.json"), queue).context("write queue.json")?;
    std::fs::write(dir.path().join(".ralph/done.json"), done).context("write done.json")?;

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
