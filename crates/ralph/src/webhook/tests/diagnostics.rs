//! Webhook diagnostics and replay tests.
//!
//! Responsibilities:
//! - Verify diagnostics snapshots, failure-store retention, and replay accounting.
//! - Assert replay selection and dry-run semantics.
//!
//! Does NOT handle:
//! - Dispatcher worker-pool concurrency behavior.
//! - Payload serialization or config parsing.
//!
//! Invariants:
//! - Tests use fixed failure records via shared support helpers.
//! - Serial tests reset dispatcher/diagnostic state before mutating global webhook runtime.

use super::super::*;
use super::support::{reset_webhook_test_state, sample_failure_record, webhook_test_config};
use crate::webhook::types::{WebhookContext, WebhookMessage, WebhookPayload};
use serial_test::serial;

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
fn replay_execute_only_increments_eligible_records() {
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
