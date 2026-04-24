//! Breakdown, filtering, and full-text stats tests.
//!
//! Purpose:
//! - Breakdown, filtering, and full-text stats tests.
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
