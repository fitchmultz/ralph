//! Integration tests for queue stats, history, and burndown report output.
//!
//! Purpose:
//! - Integration tests for queue stats, history, and burndown report output.
//!
//! Responsibilities:
//! - Provide shared CLI helpers for queue-report contract suites.
//! - Delegate summary, burndown, ETA, and breakdown coverage to focused modules.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

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

#[path = "queue_stats_history_test/breakdowns.rs"]
mod breakdowns;
#[path = "queue_stats_history_test/eta_time.rs"]
mod eta_time;
#[path = "queue_stats_history_test/history_burndown.rs"]
mod history_burndown;
#[path = "queue_stats_history_test/summary.rs"]
mod summary;
