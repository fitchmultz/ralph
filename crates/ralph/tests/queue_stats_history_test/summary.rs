//! Summary stats output tests.
//!
//! Purpose:
//! - Summary stats output tests.
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
