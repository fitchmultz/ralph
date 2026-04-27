//! Webhook diagnostics and replay tests.
//!
//! Purpose:
//! - Webhook diagnostics and replay tests.
//!
//! Responsibilities:
//! - Verify diagnostics snapshots, failure-store retention, and replay accounting.
//! - Assert replay selection and dry-run semantics.
//!
//! Non-scope:
//! - Dispatcher worker-pool concurrency behavior.
//! - Payload serialization or config parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Tests use fixed failure records via shared support helpers.
//! - Serial tests reset dispatcher/diagnostic state before mutating global webhook runtime.

use super::super::*;
use super::support::{reset_webhook_test_state, sample_failure_record, webhook_test_config};
use crate::contracts::WebhookConfig;
use crate::webhook::types::{WebhookContext, WebhookMessage, WebhookPayload};
use anyhow::{Context, Result};
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const WEBHOOK_FAILURE_STORE_HELPER_TEST: &str =
    "webhook::tests::diagnostics::webhook_failure_store_subprocess_helper";
const WEBHOOK_FAILURE_STORE_DELAY_MS: &str = "250";
const WEBHOOK_FAILURE_STORE_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const WEBHOOK_FAILURE_STORE_WAIT_SLICE: Duration = Duration::from_millis(25);
const ENV_FAILURE_STORE_HELPER: &str = "RALPH_TEST_WEBHOOK_FAILURE_STORE_HELPER";
const ENV_FAILURE_STORE_ACTION: &str = "RALPH_TEST_WEBHOOK_FAILURE_STORE_ACTION";
const ENV_FAILURE_STORE_READY_FILE: &str = "RALPH_TEST_WEBHOOK_FAILURE_STORE_READY_FILE";
const ENV_FAILURE_STORE_START_FILE: &str = "RALPH_TEST_WEBHOOK_FAILURE_STORE_START_FILE";
const ENV_FAILURE_STORE_REPO_ROOT: &str = "RALPH_TEST_WEBHOOK_FAILURE_STORE_REPO_ROOT";
const ENV_FAILURE_STORE_TASK_ID: &str = "RALPH_TEST_WEBHOOK_FAILURE_STORE_TASK_ID";
const ENV_FAILURE_STORE_REPLAY_ID: &str = "RALPH_TEST_WEBHOOK_FAILURE_STORE_REPLAY_ID";

#[test]
#[serial]
fn diagnostics_snapshot_includes_metrics_and_recent_failures() {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().expect("tempdir");
    let config = webhook_test_config();

    crate::webhook::diagnostics::set_queue_capacity(64);
    crate::webhook::diagnostics::note_enqueue_success();
    crate::webhook::diagnostics::note_enqueue_success();
    crate::webhook::diagnostics::note_queue_dequeue();
    crate::webhook::diagnostics::note_delivery_success();
    crate::webhook::diagnostics::note_retry_attempt();
    crate::webhook::diagnostics::note_dropped_message();

    crate::webhook::diagnostics::write_failure_records_for_tests(
        repo_root.path(),
        &[sample_failure_record(
            "wf-1",
            "task_completed",
            Some("RQ-0814"),
            0,
        )],
    )
    .expect("write failure store");

    let snapshot = diagnostics_snapshot(repo_root.path(), &config, 10).expect("status snapshot");

    assert_eq!(snapshot.queue_depth, 1);
    assert_eq!(snapshot.queue_capacity, 64);
    assert_eq!(snapshot.queue_policy, WebhookQueuePolicy::DropNew);
    assert_eq!(snapshot.enqueued_total, 2);
    assert_eq!(snapshot.delivered_total, 1);
    assert_eq!(snapshot.dropped_total, 1);
    assert_eq!(snapshot.retry_attempts_total, 1);
    assert_eq!(snapshot.recent_failures.len(), 1);
    assert_eq!(snapshot.recent_failures[0].id, "wf-1");
}

#[test]
#[serial]
fn diagnostics_queue_dequeue_saturates_at_zero() {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().expect("tempdir");

    crate::webhook::diagnostics::note_queue_dequeue();
    crate::webhook::diagnostics::note_queue_dequeue();

    let snapshot = diagnostics_snapshot(repo_root.path(), &WebhookConfig::default(), 10)
        .expect("status snapshot");
    assert_eq!(snapshot.queue_depth, 0);

    crate::webhook::diagnostics::note_enqueue_success();
    crate::webhook::diagnostics::note_queue_dequeue();
    crate::webhook::diagnostics::note_queue_dequeue();

    let snapshot = diagnostics_snapshot(repo_root.path(), &WebhookConfig::default(), 10)
        .expect("status snapshot");
    assert_eq!(snapshot.queue_depth, 0);
}

#[test]
#[serial]
fn diagnostics_capacity_is_zero_when_runtime_not_needed() {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().expect("tempdir");

    crate::webhook::diagnostics::set_queue_capacity(64);

    let snapshot = diagnostics_snapshot(repo_root.path(), &WebhookConfig::default(), 10)
        .expect("status snapshot");

    assert_eq!(snapshot.queue_capacity, 0);

    reset_webhook_test_state();
}

#[test]
#[serial]
fn failure_store_retention_is_bounded_to_200_records() {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().expect("tempdir");

    let msg = WebhookMessage {
        payload: WebhookPayload {
            event: "task_failed".to_string(),
            timestamp: "2026-02-13T00:00:00Z".to_string(),
            task_id: Some("RQ-0814".to_string()),
            task_title: Some("Retention test".to_string()),
            previous_status: None,
            current_status: None,
            note: None,
            context: WebhookContext::default(),
        },
        config: ResolvedWebhookConfig::from_config(&webhook_test_config()),
    };

    for _ in 0..205 {
        crate::webhook::diagnostics::persist_failed_delivery_for_tests(
            repo_root.path(),
            &msg,
            &anyhow::anyhow!("simulated failure"),
            1,
        )
        .expect("persist failed delivery");
    }

    let records = crate::webhook::diagnostics::load_failure_records_for_tests(repo_root.path())
        .expect("load failure records");
    assert_eq!(records.len(), 200);
}

#[test]
#[serial]
fn failure_store_runtime_persistence_uses_payload_repo_root_not_cwd() -> Result<()> {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().context("create payload repo root")?;
    let other_cwd = tempfile::tempdir().context("create unrelated cwd")?;
    let previous_cwd = std::env::current_dir().context("capture current directory")?;
    std::env::set_current_dir(other_cwd.path()).context("switch to unrelated cwd")?;

    let result = (|| {
        let expected_repo_root = repo_root.path().display().to_string();
        let msg = WebhookMessage {
            payload: WebhookPayload {
                event: "task_completed".to_string(),
                timestamp: "2026-02-13T00:00:00Z".to_string(),
                task_id: Some("RQ-0047".to_string()),
                task_title: Some("Repo context propagation".to_string()),
                previous_status: Some("doing".to_string()),
                current_status: Some("done".to_string()),
                note: None,
                context: WebhookContext {
                    repo_root: Some(expected_repo_root.clone()),
                    branch: Some("main".to_string()),
                    commit: Some("abc123".to_string()),
                    ..Default::default()
                },
            },
            config: ResolvedWebhookConfig::from_config(&webhook_test_config()),
        };

        crate::webhook::diagnostics::persist_failed_delivery_from_runtime_for_tests(
            &msg,
            &anyhow::anyhow!("simulated delivery failure"),
            1,
        )?;

        let records = crate::webhook::diagnostics::load_failure_records_for_tests(repo_root.path())
            .context("load payload-root failure records")?;
        anyhow::ensure!(
            records.len() == 1,
            "expected one payload-root failure record"
        );
        anyhow::ensure!(
            records[0].payload.context.repo_root.as_deref()
                == Some(crate::constants::defaults::REDACTED),
            "expected persisted payload to redact repo_root"
        );
        anyhow::ensure!(
            !other_cwd
                .path()
                .join(".ralph/cache/webhooks/failures.json")
                .exists(),
            "cwd fallback should not create an unrelated failure store"
        );
        Ok(())
    })();

    let restore = std::env::set_current_dir(&previous_cwd).context("restore current directory");
    result.and(restore)
}

#[test]
#[serial]
fn failure_store_runtime_persistence_skips_missing_repo_root_without_cwd_fallback() -> Result<()> {
    reset_webhook_test_state();
    let other_cwd = tempfile::tempdir().context("create unrelated cwd")?;
    let previous_cwd = std::env::current_dir().context("capture current directory")?;
    std::env::set_current_dir(other_cwd.path()).context("switch to unrelated cwd")?;

    let result = (|| {
        let msg = WebhookMessage {
            payload: WebhookPayload {
                event: "task_failed".to_string(),
                timestamp: "2026-02-13T00:00:00Z".to_string(),
                task_id: Some("RQ-0047".to_string()),
                task_title: Some("Missing context".to_string()),
                previous_status: Some("doing".to_string()),
                current_status: Some("rejected".to_string()),
                note: None,
                context: WebhookContext::default(),
            },
            config: ResolvedWebhookConfig::from_config(&webhook_test_config()),
        };

        crate::webhook::diagnostics::persist_failed_delivery_from_runtime_for_tests(
            &msg,
            &anyhow::anyhow!("simulated delivery failure"),
            1,
        )?;

        anyhow::ensure!(
            !other_cwd
                .path()
                .join(".ralph/cache/webhooks/failures.json")
                .exists(),
            "missing repo_root should not create a failure store in cwd"
        );
        Ok(())
    })();

    let restore = std::env::set_current_dir(&previous_cwd).context("restore current directory");
    result.and(restore)
}

#[test]
fn replay_selector_filtering_and_cap_behavior() {
    let repo_root = tempfile::tempdir().expect("tempdir");
    let config = webhook_test_config();

    crate::webhook::diagnostics::write_failure_records_for_tests(
        repo_root.path(),
        &[
            sample_failure_record("wf-a", "task_completed", Some("RQ-0814"), 0),
            sample_failure_record("wf-b", "task_completed", Some("RQ-0815"), 2),
            sample_failure_record("wf-c", "task_failed", Some("RQ-0814"), 0),
        ],
    )
    .expect("write failure records");

    let report = replay_failed_deliveries(
        repo_root.path(),
        &config,
        &ReplaySelector {
            ids: Vec::new(),
            event: Some("task_completed".to_string()),
            task_id: None,
            limit: 10,
            max_replay_attempts: 2,
        },
        true,
    )
    .expect("replay dry-run report");

    assert!(report.dry_run);
    assert_eq!(report.matched_count, 2);
    assert_eq!(report.eligible_count, 1);
    assert_eq!(report.skipped_max_replay_attempts, 1);
}

#[test]
fn replay_dry_run_does_not_mutate_replay_counts() {
    let repo_root = tempfile::tempdir().expect("tempdir");
    let config = webhook_test_config();
    crate::webhook::diagnostics::write_failure_records_for_tests(
        repo_root.path(),
        &[sample_failure_record(
            "wf-dry",
            "task_completed",
            Some("RQ-0814"),
            0,
        )],
    )
    .expect("write failure records");

    let report = replay_failed_deliveries(
        repo_root.path(),
        &config,
        &ReplaySelector {
            ids: vec!["wf-dry".to_string()],
            event: None,
            task_id: None,
            limit: 10,
            max_replay_attempts: 3,
        },
        true,
    )
    .expect("dry-run replay");

    assert_eq!(report.matched_count, 1);
    assert_eq!(report.replayed_count, 0);

    let records = crate::webhook::diagnostics::load_failure_records_for_tests(repo_root.path())
        .expect("reload failure records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].replay_count, 0);
}

#[test]
#[serial]
fn replay_execute_only_increments_eligible_records() {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().expect("tempdir");
    let config = webhook_test_config();
    crate::webhook::diagnostics::write_failure_records_for_tests(
        repo_root.path(),
        &[
            sample_failure_record("wf-live", "task_completed", Some("RQ-0814"), 0),
            sample_failure_record("wf-capped", "task_completed", Some("RQ-0814"), 3),
        ],
    )
    .expect("write failure records");

    let report = replay_failed_deliveries(
        repo_root.path(),
        &config,
        &ReplaySelector {
            ids: Vec::new(),
            event: Some("task_completed".to_string()),
            task_id: None,
            limit: 10,
            max_replay_attempts: 3,
        },
        false,
    )
    .expect("replay execution report");

    assert!(!report.dry_run);
    assert_eq!(report.matched_count, 2);
    assert_eq!(report.eligible_count, 1);
    assert_eq!(report.replayed_count, 1);
    assert_eq!(report.skipped_max_replay_attempts, 1);

    let records = crate::webhook::diagnostics::load_failure_records_for_tests(repo_root.path())
        .expect("reload failure records");
    let live = records
        .iter()
        .find(|record| record.id == "wf-live")
        .unwrap();
    assert_eq!(live.replay_count, 1);
    let capped = records
        .iter()
        .find(|record| record.id == "wf-capped")
        .unwrap();
    assert_eq!(capped.replay_count, 3);
}

#[test]
#[serial]
fn failure_store_cross_process_persistence_preserves_all_records() -> Result<()> {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().context("create temp repo root")?;
    let start_file = repo_root.path().join("persist.start");
    let ready_one = repo_root.path().join("persist-worker-1.ready");
    let ready_two = repo_root.path().join("persist-worker-2.ready");

    let mut child_one = spawn_failure_store_helper(
        repo_root.path(),
        "persist",
        &ready_one,
        &start_file,
        Some("RQ-1001"),
        None,
    )?;
    let mut child_two = spawn_failure_store_helper(
        repo_root.path(),
        "persist",
        &ready_two,
        &start_file,
        Some("RQ-1002"),
        None,
    )?;

    wait_for_paths(&[&ready_one, &ready_two])?;
    fs::write(&start_file, b"go").context("release persistence helpers")?;
    wait_for_child_success(&mut child_one, "persist worker 1")?;
    wait_for_child_success(&mut child_two, "persist worker 2")?;

    let records = crate::webhook::diagnostics::load_failure_records_for_tests(repo_root.path())
        .context("reload failure records")?;
    assert_eq!(records.len(), 2);

    let task_ids = records
        .iter()
        .map(|record| record.task_id.as_deref().unwrap_or_default())
        .collect::<std::collections::HashSet<_>>();
    assert!(task_ids.contains("RQ-1001"));
    assert!(task_ids.contains("RQ-1002"));
    for record in &records {
        assert_eq!(
            record.destination.as_deref(),
            Some("https://hooks.example.com/…")
        );
        assert!(
            !record.error.contains("token=supersecret"),
            "expected redacted error, got: {}",
            record.error
        );
    }

    Ok(())
}

#[test]
#[serial]
fn replay_count_updates_are_preserved_across_processes() -> Result<()> {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().context("create temp repo root")?;
    let start_file = repo_root.path().join("replay.start");
    let ready_one = repo_root.path().join("replay-worker-1.ready");
    let ready_two = repo_root.path().join("replay-worker-2.ready");

    crate::webhook::diagnostics::write_failure_records_for_tests(
        repo_root.path(),
        &[sample_failure_record(
            "wf-race",
            "task_completed",
            Some("RQ-0814"),
            0,
        )],
    )
    .context("seed failure store")?;

    let mut child_one = spawn_failure_store_helper(
        repo_root.path(),
        "replay",
        &ready_one,
        &start_file,
        None,
        Some("wf-race"),
    )?;
    let mut child_two = spawn_failure_store_helper(
        repo_root.path(),
        "replay",
        &ready_two,
        &start_file,
        None,
        Some("wf-race"),
    )?;

    wait_for_paths(&[&ready_one, &ready_two])?;
    fs::write(&start_file, b"go").context("release replay helpers")?;
    wait_for_child_success(&mut child_one, "replay worker 1")?;
    wait_for_child_success(&mut child_two, "replay worker 2")?;

    let records = crate::webhook::diagnostics::load_failure_records_for_tests(repo_root.path())
        .context("reload failure records")?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].replay_count, 2);

    Ok(())
}

#[test]
#[serial]
fn webhook_failure_store_subprocess_helper() -> Result<()> {
    if std::env::var(ENV_FAILURE_STORE_HELPER).ok().as_deref() != Some("1") {
        return Ok(());
    }

    let repo_root =
        PathBuf::from(std::env::var(ENV_FAILURE_STORE_REPO_ROOT).context("read repo-root env")?);
    let ready_file =
        PathBuf::from(std::env::var(ENV_FAILURE_STORE_READY_FILE).context("read ready-file env")?);
    let start_file =
        PathBuf::from(std::env::var(ENV_FAILURE_STORE_START_FILE).context("read start-file env")?);
    let action = std::env::var(ENV_FAILURE_STORE_ACTION).context("read action env")?;

    fs::write(&ready_file, b"ready").context("write ready file")?;
    wait_for_paths(&[start_file.as_path()])?;

    match action.as_str() {
        "persist" => persist_failed_delivery_for_subprocess(&repo_root),
        "replay" => update_replay_count_for_subprocess(&repo_root),
        other => Err(anyhow::anyhow!("unknown helper action: {other}")),
    }
}

fn spawn_failure_store_helper(
    repo_root: &Path,
    action: &str,
    ready_file: &Path,
    start_file: &Path,
    task_id: Option<&str>,
    replay_id: Option<&str>,
) -> Result<Child> {
    let mut command =
        Command::new(std::env::current_exe().context("resolve current test executable path")?);
    command
        .arg("--exact")
        .arg(WEBHOOK_FAILURE_STORE_HELPER_TEST)
        .arg("--nocapture")
        .env(ENV_FAILURE_STORE_HELPER, "1")
        .env(ENV_FAILURE_STORE_ACTION, action)
        .env(ENV_FAILURE_STORE_READY_FILE, ready_file)
        .env(ENV_FAILURE_STORE_START_FILE, start_file)
        .env(ENV_FAILURE_STORE_REPO_ROOT, repo_root)
        .env(
            "RALPH_TEST_WEBHOOK_FAILURE_STORE_DELAY_MS",
            WEBHOOK_FAILURE_STORE_DELAY_MS,
        )
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    if let Some(task_id) = task_id {
        command.env(ENV_FAILURE_STORE_TASK_ID, task_id);
    }
    if let Some(replay_id) = replay_id {
        command.env(ENV_FAILURE_STORE_REPLAY_ID, replay_id);
    }

    command.spawn().context("spawn failure-store helper")
}

fn persist_failed_delivery_for_subprocess(repo_root: &Path) -> Result<()> {
    let task_id = std::env::var(ENV_FAILURE_STORE_TASK_ID).context("read task-id env")?;
    let mut config = webhook_test_config();
    config.url = Some("https://hooks.example.com/delivery?token=supersecret".to_string());

    let msg = WebhookMessage {
        payload: WebhookPayload {
            event: "task_failed".to_string(),
            timestamp: "2026-02-13T00:00:00Z".to_string(),
            task_id: Some(task_id.clone()),
            task_title: Some(format!("Webhook race {task_id}")),
            previous_status: None,
            current_status: None,
            note: None,
            context: WebhookContext::default(),
        },
        config: ResolvedWebhookConfig::from_config(&config),
    };

    crate::webhook::diagnostics::persist_failed_delivery_for_tests(
        repo_root,
        &msg,
        &anyhow::anyhow!("delivery to https://hooks.example.com/delivery?token=supersecret failed"),
        1,
    )
}

fn update_replay_count_for_subprocess(repo_root: &Path) -> Result<()> {
    let replay_id = std::env::var(ENV_FAILURE_STORE_REPLAY_ID).context("read replay-id env")?;
    crate::webhook::diagnostics::update_replay_counts_for_tests(repo_root, &[replay_id])
}

fn wait_for_paths(paths: &[&Path]) -> Result<()> {
    let deadline = Instant::now() + WEBHOOK_FAILURE_STORE_WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if paths.iter().all(|path| path.exists()) {
            return Ok(());
        }
        std::thread::sleep(WEBHOOK_FAILURE_STORE_WAIT_SLICE);
    }

    Err(anyhow::anyhow!(
        "timed out waiting for test paths: {}",
        paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn wait_for_child_success(child: &mut Child, label: &str) -> Result<()> {
    let status = child
        .wait()
        .with_context(|| format!("wait for {label} to exit"))?;
    anyhow::ensure!(status.success(), "{label} exited with status {status}");
    Ok(())
}
