//! Integration tests for `ralph webhook` diagnostics and replay commands.
//!
//! Purpose:
//! - Integration tests for `ralph webhook` diagnostics and replay commands.
//!
//! Responsibilities:
//! - Verify `webhook status --format json` emits parseable diagnostics.
//! - Verify replay safety guardrails require explicit selectors.
//! - Verify replay dry-run is non-mutating.
//! - Verify replay execution updates persisted replay metadata.
//!
//! Not handled here:
//! - End-to-end webhook network reliability (covered in webhook delivery integration tests).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests run in isolated temp repos initialized with `ralph init --non-interactive`.
//! - Failure records are stored under `.ralph/cache/webhooks/failures.json`.

use anyhow::{Context, Result};
use ralph::webhook::{WebhookContext, WebhookFailureRecord, WebhookPayload};
use serde_json::Value;

mod test_support;

fn setup_repo() -> Result<tempfile::TempDir> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;
    Ok(dir)
}

fn write_webhook_config(dir: &std::path::Path) -> Result<()> {
    let config_path = dir.join(".ralph/config.jsonc");
    let mut config: Value = serde_json::from_str(
        &std::fs::read_to_string(&config_path)
            .with_context(|| format!("read {}", config_path.display()))?,
    )?;

    config["agent"]["webhook"] = serde_json::json!({
        "enabled": true,
        "url": "http://127.0.0.1:9/webhook",
        "allow_insecure_http": true,
        "allow_private_targets": true,
        "events": ["*"],
        "retry_count": 0,
        "retry_backoff_ms": 100,
        "queue_capacity": 100,
        "queue_policy": "drop_new"
    });

    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)
        .with_context(|| format!("write {}", config_path.display()))?;
    Ok(())
}

fn failure_store_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join(".ralph/cache/webhooks/failures.json")
}

fn sample_failure(id: &str, replay_count: u32) -> WebhookFailureRecord {
    WebhookFailureRecord {
        id: id.to_string(),
        failed_at: "2026-02-13T00:00:00Z".to_string(),
        event: "task_completed".to_string(),
        task_id: Some("RQ-0814".to_string()),
        destination: Some("http://127.0.0.1:9/…".to_string()),
        error: "HTTP 500: endpoint failed".to_string(),
        attempts: 3,
        replay_count,
        payload: WebhookPayload {
            event: "task_completed".to_string(),
            timestamp: "2026-02-13T00:00:00Z".to_string(),
            task_id: Some("RQ-0814".to_string()),
            task_title: Some("Webhook replay test".to_string()),
            previous_status: Some("doing".to_string()),
            current_status: Some("done".to_string()),
            note: Some("integration test".to_string()),
            context: WebhookContext::default(),
        },
    }
}

fn write_failure_store(dir: &std::path::Path, records: &[WebhookFailureRecord]) -> Result<()> {
    let path = failure_store_path(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(records)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn read_failure_store(dir: &std::path::Path) -> Result<Vec<WebhookFailureRecord>> {
    let path = failure_store_path(dir);
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let parsed = serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    Ok(parsed)
}

#[test]
fn webhook_status_json_is_parseable() -> Result<()> {
    let dir = setup_repo()?;
    write_webhook_config(dir.path())?;
    write_failure_store(dir.path(), &[sample_failure("wf-status", 0)])?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["webhook", "status", "--format", "json"]);
    anyhow::ensure!(
        status.success(),
        "status command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let output: Value = serde_json::from_str(&stdout).context("parse status json")?;
    anyhow::ensure!(output.get("queue_depth").is_some(), "missing queue_depth");
    anyhow::ensure!(
        output.get("queue_capacity").is_some(),
        "missing queue_capacity"
    );
    anyhow::ensure!(
        output.get("recent_failures").is_some(),
        "missing recent_failures"
    );

    let failures = output["recent_failures"]
        .as_array()
        .expect("recent_failures should be an array");
    anyhow::ensure!(failures.len() == 1, "expected one seeded failure");
    anyhow::ensure!(
        failures[0]["id"] == "wf-status",
        "expected seeded record id in status output"
    );
    Ok(())
}

#[test]
fn webhook_replay_requires_selector() -> Result<()> {
    let dir = setup_repo()?;
    write_webhook_config(dir.path())?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["webhook", "replay", "--dry-run"]);

    anyhow::ensure!(
        !status.success(),
        "expected replay to fail without selector"
    );
    anyhow::ensure!(
        stderr.contains("Refusing broad replay"),
        "missing selector guard error: {stderr}"
    );
    Ok(())
}

#[test]
fn webhook_replay_dry_run_does_not_mutate_store() -> Result<()> {
    let dir = setup_repo()?;
    write_webhook_config(dir.path())?;
    write_failure_store(dir.path(), &[sample_failure("wf-dry", 1)])?;

    let before = std::fs::read_to_string(failure_store_path(dir.path()))?;
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["webhook", "replay", "--dry-run", "--id", "wf-dry"],
    );
    anyhow::ensure!(
        status.success(),
        "dry-run replay failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let after = std::fs::read_to_string(failure_store_path(dir.path()))?;

    anyhow::ensure!(before == after, "dry-run replay mutated failure store");
    Ok(())
}

#[test]
fn webhook_replay_updates_replay_count_for_selected_id() -> Result<()> {
    let dir = setup_repo()?;
    write_webhook_config(dir.path())?;
    write_failure_store(dir.path(), &[sample_failure("wf-replay", 0)])?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["webhook", "replay", "--id", "wf-replay"]);
    anyhow::ensure!(
        status.success(),
        "replay command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let records = read_failure_store(dir.path())?;
    let updated = records
        .iter()
        .find(|record| record.id == "wf-replay")
        .expect("expected selected record to remain present");

    anyhow::ensure!(
        updated.replay_count == 1,
        "expected replay_count to increment to 1, got {}",
        updated.replay_count
    );

    Ok(())
}

#[test]
fn webhook_replay_event_filter_respects_replay_caps() -> Result<()> {
    let dir = setup_repo()?;
    write_webhook_config(dir.path())?;

    let mut non_matching = sample_failure("wf-other-event", 0);
    non_matching.event = "task_failed".to_string();
    non_matching.payload.event = "task_failed".to_string();

    write_failure_store(
        dir.path(),
        &[
            sample_failure("wf-eligible", 0),
            sample_failure("wf-capped", 3),
            non_matching,
        ],
    )?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "webhook",
            "replay",
            "--event",
            "task_completed",
            "--max-replay-attempts",
            "3",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "event replay command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let records = read_failure_store(dir.path())?;
    let eligible = records
        .iter()
        .find(|record| record.id == "wf-eligible")
        .expect("wf-eligible should exist");
    let capped = records
        .iter()
        .find(|record| record.id == "wf-capped")
        .expect("wf-capped should exist");
    let other = records
        .iter()
        .find(|record| record.id == "wf-other-event")
        .expect("wf-other-event should exist");

    anyhow::ensure!(
        eligible.replay_count == 1,
        "eligible record should increment to 1, got {}",
        eligible.replay_count
    );
    anyhow::ensure!(
        capped.replay_count == 3,
        "capped record should remain at 3, got {}",
        capped.replay_count
    );
    anyhow::ensure!(
        other.replay_count == 0,
        "non-matching record should remain unchanged, got {}",
        other.replay_count
    );
    Ok(())
}
