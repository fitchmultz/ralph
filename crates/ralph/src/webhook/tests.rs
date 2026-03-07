//! Webhook unit tests.
//!
//! Responsibilities:
//! - Validate webhook payload serialization, event type parsing, and config defaults.
//! - Validate queue policy parsing and merge behavior.
//!
//! Does NOT handle:
//! - Network delivery behavior (see integration tests).
//! - Cryptographic verification beyond signature format.
//!
//! Invariants/assumptions:
//! - Tests may access private module helpers via `super::*`.

use super::*;
use crate::contracts::WebhookConfig;
use crate::webhook::types::{WebhookContext, WebhookMessage, WebhookPayload};
use crate::webhook::worker::{
    current_dispatcher_settings_for_tests, generate_signature, install_test_transport_for_tests,
    redact_webhook_destination, reset_dispatcher_for_tests,
};
use crossbeam_channel::bounded;
use serial_test::serial;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn sample_failure_record(
    id: &str,
    event: &str,
    task_id: Option<&str>,
    replay_count: u32,
) -> WebhookFailureRecord {
    WebhookFailureRecord {
        id: id.to_string(),
        failed_at: "2026-02-13T00:00:00Z".to_string(),
        event: event.to_string(),
        task_id: task_id.map(std::string::ToString::to_string),
        destination: Some("https://hooks.example.com/…".to_string()),
        error: "HTTP 500: endpoint failed".to_string(),
        attempts: 3,
        replay_count,
        payload: WebhookPayload {
            event: event.to_string(),
            timestamp: "2026-02-13T00:00:00Z".to_string(),
            task_id: task_id.map(std::string::ToString::to_string),
            task_title: Some("Test task".to_string()),
            previous_status: None,
            current_status: None,
            note: None,
            context: WebhookContext::default(),
        },
    }
}

fn reset_webhook_test_state() {
    super::diagnostics::reset_webhook_metrics_for_tests();
    reset_dispatcher_for_tests();
}

fn webhook_test_config() -> WebhookConfig {
    use crate::contracts::WebhookEventSubscription;
    WebhookConfig {
        enabled: Some(true),
        url: Some("http://127.0.0.1:9/webhook".to_string()),
        secret: None,
        events: Some(vec![WebhookEventSubscription::Wildcard]),
        timeout_secs: Some(1),
        retry_count: Some(0),
        retry_backoff_ms: Some(1),
        queue_capacity: Some(100),
        parallel_queue_multiplier: None,
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    }
}

#[test]
fn webhook_event_type_as_str() {
    assert_eq!(WebhookEventType::TaskCreated.as_str(), "task_created");
    assert_eq!(WebhookEventType::TaskStarted.as_str(), "task_started");
    assert_eq!(WebhookEventType::TaskCompleted.as_str(), "task_completed");
    assert_eq!(WebhookEventType::TaskFailed.as_str(), "task_failed");
    assert_eq!(
        WebhookEventType::TaskStatusChanged.as_str(),
        "task_status_changed"
    );
    assert_eq!(WebhookEventType::LoopStarted.as_str(), "loop_started");
    assert_eq!(WebhookEventType::LoopStopped.as_str(), "loop_stopped");
    assert_eq!(WebhookEventType::PhaseStarted.as_str(), "phase_started");
    assert_eq!(WebhookEventType::PhaseCompleted.as_str(), "phase_completed");
    assert_eq!(WebhookEventType::QueueUnblocked.as_str(), "queue_unblocked");
}

#[test]
fn webhook_event_type_from_str() {
    use std::str::FromStr;

    assert_eq!(
        WebhookEventType::from_str("task_created").unwrap(),
        WebhookEventType::TaskCreated
    );
    assert_eq!(
        WebhookEventType::from_str("task_started").unwrap(),
        WebhookEventType::TaskStarted
    );
    assert_eq!(
        WebhookEventType::from_str("task_completed").unwrap(),
        WebhookEventType::TaskCompleted
    );
    assert_eq!(
        WebhookEventType::from_str("task_failed").unwrap(),
        WebhookEventType::TaskFailed
    );
    assert_eq!(
        WebhookEventType::from_str("task_status_changed").unwrap(),
        WebhookEventType::TaskStatusChanged
    );
    assert_eq!(
        WebhookEventType::from_str("loop_started").unwrap(),
        WebhookEventType::LoopStarted
    );
    assert_eq!(
        WebhookEventType::from_str("loop_stopped").unwrap(),
        WebhookEventType::LoopStopped
    );
    assert_eq!(
        WebhookEventType::from_str("phase_started").unwrap(),
        WebhookEventType::PhaseStarted
    );
    assert_eq!(
        WebhookEventType::from_str("phase_completed").unwrap(),
        WebhookEventType::PhaseCompleted
    );
    assert_eq!(
        WebhookEventType::from_str("queue_unblocked").unwrap(),
        WebhookEventType::QueueUnblocked
    );

    // Unknown event should error
    assert!(WebhookEventType::from_str("unknown_event").is_err());
}

#[test]
fn resolved_config_defaults() {
    let config = WebhookConfig::default();
    let resolved = ResolvedWebhookConfig::from_config(&config);

    assert!(!resolved.enabled);
    assert_eq!(resolved.timeout, Duration::from_secs(30));
    assert_eq!(resolved.retry_count, 3);
    assert_eq!(resolved.retry_backoff, Duration::from_millis(1000));
}

#[test]
fn is_event_enabled_legacy_defaults_only() {
    // When events is None, only legacy task events are enabled (new events are opt-in)
    let config = WebhookConfig {
        enabled: Some(true),
        ..Default::default()
    };

    // Legacy task events should be enabled
    assert!(config.is_event_enabled("task_created"));
    assert!(config.is_event_enabled("task_started"));
    assert!(config.is_event_enabled("task_completed"));
    assert!(config.is_event_enabled("task_failed"));
    assert!(config.is_event_enabled("task_status_changed"));

    // New loop/phase events should NOT be enabled by default (opt-in)
    assert!(!config.is_event_enabled("loop_started"));
    assert!(!config.is_event_enabled("loop_stopped"));
    assert!(!config.is_event_enabled("phase_started"));
    assert!(!config.is_event_enabled("phase_completed"));

    // Unknown events should not be enabled
    assert!(!config.is_event_enabled("any_event"));
    assert!(!config.is_event_enabled("custom_event"));
}

#[test]
fn is_event_enabled_with_specific_events() {
    use crate::contracts::WebhookEventSubscription;
    let config = WebhookConfig {
        enabled: Some(true),
        events: Some(vec![
            WebhookEventSubscription::TaskCreated,
            WebhookEventSubscription::TaskCompleted,
        ]),
        ..Default::default()
    };

    assert!(config.is_event_enabled("task_created"));
    assert!(config.is_event_enabled("task_completed"));
    assert!(!config.is_event_enabled("task_started"));
}

#[test]
fn is_event_enabled_wildcard_subscribes_to_all() {
    use crate::contracts::WebhookEventSubscription;
    // Using ["*"] should enable all events including new ones
    let config = WebhookConfig {
        enabled: Some(true),
        events: Some(vec![WebhookEventSubscription::Wildcard]),
        ..Default::default()
    };

    // Legacy events
    assert!(config.is_event_enabled("task_created"));
    assert!(config.is_event_enabled("task_started"));
    assert!(config.is_event_enabled("task_completed"));

    // New loop/phase events
    assert!(config.is_event_enabled("loop_started"));
    assert!(config.is_event_enabled("loop_stopped"));
    assert!(config.is_event_enabled("phase_started"));
    assert!(config.is_event_enabled("phase_completed"));

    // Unknown/custom events also enabled with wildcard
    assert!(config.is_event_enabled("any_event"));
    assert!(config.is_event_enabled("custom_event"));
}

#[test]
fn is_event_enabled_opt_in_new_events() {
    use crate::contracts::WebhookEventSubscription;
    // Explicitly opt-in to new events
    let config = WebhookConfig {
        enabled: Some(true),
        events: Some(vec![
            WebhookEventSubscription::TaskCompleted,
            WebhookEventSubscription::PhaseCompleted,
            WebhookEventSubscription::LoopStarted,
        ]),
        ..Default::default()
    };

    assert!(config.is_event_enabled("task_completed"));
    assert!(config.is_event_enabled("phase_completed"));
    assert!(config.is_event_enabled("loop_started"));

    // Not opted-in
    assert!(!config.is_event_enabled("task_created"));
    assert!(!config.is_event_enabled("phase_started"));
    assert!(!config.is_event_enabled("loop_stopped"));
}

#[test]
fn is_event_enabled_disabled_globally() {
    use crate::contracts::WebhookEventSubscription;
    let config = WebhookConfig {
        enabled: Some(false),
        events: Some(vec![WebhookEventSubscription::Wildcard]),
        ..Default::default()
    };

    assert!(!config.is_event_enabled("task_created"));
}

#[test]
fn generate_signature_format() {
    let body = r#"{"event":"test","task_id":"RQ-0001"}"#;
    let secret = "my-secret-key";
    let sig = generate_signature(body, secret);

    assert!(sig.starts_with("sha256="));
    assert_eq!(sig.len(), 7 + 64); // "sha256=" + 64 hex chars
}

#[test]
fn payload_serialization() {
    let payload = WebhookPayload {
        event: "task_created".to_string(),
        timestamp: "2024-01-15T10:30:00Z".to_string(),
        task_id: Some("RQ-0001".to_string()),
        task_title: Some("Test task".to_string()),
        previous_status: None,
        current_status: Some("todo".to_string()),
        note: None,
        context: WebhookContext::default(),
    };

    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains("\"event\":\"task_created\""));
    assert!(json.contains("\"task_id\":\"RQ-0001\""));
    assert!(!json.contains("previous_status")); // skipped when None
    assert!(!json.contains("runner")); // context fields skipped when None
}

#[test]
fn payload_serialization_with_context() {
    let payload = WebhookPayload {
        event: "phase_completed".to_string(),
        timestamp: "2024-01-15T10:30:00Z".to_string(),
        task_id: Some("RQ-0001".to_string()),
        task_title: Some("Test task".to_string()),
        previous_status: None,
        current_status: None,
        note: None,
        context: WebhookContext {
            runner: Some("claude".to_string()),
            model: Some("sonnet".to_string()),
            phase: Some(2),
            phase_count: Some(3),
            duration_ms: Some(12500),
            repo_root: Some("/home/user/project".to_string()),
            branch: Some("main".to_string()),
            commit: Some("abc123".to_string()),
            ci_gate: Some("passed".to_string()),
        },
    };

    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains("\"event\":\"phase_completed\""));
    assert!(json.contains("\"runner\":\"claude\""));
    assert!(json.contains("\"model\":\"sonnet\""));
    assert!(json.contains("\"phase\":2"));
    assert!(json.contains("\"phase_count\":3"));
    assert!(json.contains("\"duration_ms\":12500"));
    assert!(json.contains("\"repo_root\":\"/home/user/project\""));
    assert!(json.contains("\"branch\":\"main\""));
    assert!(json.contains("\"commit\":\"abc123\""));
    assert!(json.contains("\"ci_gate\":\"passed\""));
}

#[test]
fn payload_serialization_loop_event() {
    // Loop events don't have task_id/task_title
    let payload = WebhookPayload {
        event: "loop_started".to_string(),
        timestamp: "2024-01-15T10:30:00Z".to_string(),
        task_id: None,
        task_title: None,
        previous_status: None,
        current_status: None,
        note: None,
        context: WebhookContext {
            repo_root: Some("/home/user/project".to_string()),
            branch: Some("main".to_string()),
            ..Default::default()
        },
    };

    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains("\"event\":\"loop_started\""));
    assert!(json.contains("\"repo_root\":\"/home/user/project\""));
    assert!(json.contains("\"branch\":\"main\""));
    // task_id and task_title should be absent
    assert!(!json.contains("task_id"));
    assert!(!json.contains("task_title"));
}

#[test]
fn webhook_queue_policy_default() {
    let policy: WebhookQueuePolicy = Default::default();
    assert_eq!(policy, WebhookQueuePolicy::DropOldest);
}

#[test]
fn webhook_queue_policy_deserialization() {
    // Test drop_oldest
    let config_json = r#"{"queue_policy": "drop_oldest"}"#;
    let config: WebhookConfig = serde_json::from_str(config_json).unwrap();
    assert_eq!(config.queue_policy, Some(WebhookQueuePolicy::DropOldest));

    // Test drop_new
    let config_json = r#"{"queue_policy": "drop_new"}"#;
    let config: WebhookConfig = serde_json::from_str(config_json).unwrap();
    assert_eq!(config.queue_policy, Some(WebhookQueuePolicy::DropNew));

    // Test block_with_timeout
    let config_json = r#"{"queue_policy": "block_with_timeout"}"#;
    let config: WebhookConfig = serde_json::from_str(config_json).unwrap();
    assert_eq!(
        config.queue_policy,
        Some(WebhookQueuePolicy::BlockWithTimeout)
    );
}

#[test]
fn webhook_config_queue_defaults() {
    let config = WebhookConfig::default();
    assert_eq!(config.queue_capacity, None);
    assert_eq!(config.queue_policy, None);
}

#[test]
fn webhook_config_queue_capacity_parsing() {
    let config_json = r#"{"queue_capacity": 500}"#;
    let config: WebhookConfig = serde_json::from_str(config_json).unwrap();
    assert_eq!(config.queue_capacity, Some(500));
}

#[test]
fn webhook_config_merge_includes_queue_fields() {
    let mut base = WebhookConfig {
        queue_capacity: Some(100),
        queue_policy: Some(WebhookQueuePolicy::DropOldest),
        ..Default::default()
    };

    let other = WebhookConfig {
        queue_capacity: Some(200),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
        ..Default::default()
    };

    base.merge_from(other);

    assert_eq!(base.queue_capacity, Some(200));
    assert_eq!(base.queue_policy, Some(WebhookQueuePolicy::DropNew));
}

#[test]
fn webhook_queue_capacity_bounds_check() {
    // Test that capacity is properly bounded (clamped to 1-10000 range)
    // Zero would create a rendezvous channel where all sends fail
    let low_config = WebhookConfig {
        queue_capacity: Some(0),
        ..Default::default()
    };
    let capacity = low_config
        .queue_capacity
        .map(|c| c.clamp(1u32, 10000))
        .unwrap_or(100);
    assert_eq!(capacity, 1, "Capacity should be clamped to minimum of 1");

    let high_config = WebhookConfig {
        queue_capacity: Some(50000),
        ..Default::default()
    };
    let capacity = high_config
        .queue_capacity
        .map(|c| c.clamp(1u32, 10000))
        .unwrap_or(100);
    assert_eq!(
        capacity, 10000,
        "Capacity should be clamped to maximum of 10000"
    );

    let normal_config = WebhookConfig {
        queue_capacity: Some(500),
        ..Default::default()
    };
    let capacity = normal_config
        .queue_capacity
        .map(|c| c.clamp(1u32, 10000))
        .unwrap_or(100);
    assert_eq!(capacity, 500, "Normal capacity should be preserved");
}

#[test]
#[serial]
fn diagnostics_snapshot_includes_metrics_and_recent_failures() {
    super::diagnostics::reset_webhook_metrics_for_tests();
    let repo_root = tempfile::tempdir().expect("tempdir");
    let config = webhook_test_config();

    super::diagnostics::set_queue_capacity(64);
    super::diagnostics::note_enqueue_success();
    super::diagnostics::note_enqueue_success();
    super::diagnostics::note_queue_dequeue();
    super::diagnostics::note_delivery_success();
    super::diagnostics::note_retry_attempt();
    super::diagnostics::note_dropped_message();

    super::diagnostics::write_failure_records_for_tests(
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
    super::diagnostics::reset_webhook_metrics_for_tests();
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
        super::diagnostics::persist_failed_delivery_for_tests(
            repo_root.path(),
            &msg,
            &anyhow::anyhow!("simulated failure"),
            1,
        )
        .expect("persist failed delivery");
    }

    let records = super::diagnostics::load_failure_records_for_tests(repo_root.path())
        .expect("load failure records");
    assert_eq!(records.len(), 200);
}

#[test]
fn replay_selector_filtering_and_cap_behavior() {
    let repo_root = tempfile::tempdir().expect("tempdir");
    let config = webhook_test_config();

    super::diagnostics::write_failure_records_for_tests(
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
    super::diagnostics::write_failure_records_for_tests(
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

    let records = super::diagnostics::load_failure_records_for_tests(repo_root.path())
        .expect("reload failure records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].replay_count, 0);
}

#[test]
fn replay_execute_only_increments_eligible_records() {
    let repo_root = tempfile::tempdir().expect("tempdir");
    let config = webhook_test_config();
    super::diagnostics::write_failure_records_for_tests(
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

    let records = super::diagnostics::load_failure_records_for_tests(repo_root.path())
        .expect("reload failure records");
    let live = records
        .iter()
        .find(|record| record.id == "wf-live")
        .expect("wf-live record should exist");
    assert_eq!(live.replay_count, 1);
    let capped = records
        .iter()
        .find(|record| record.id == "wf-capped")
        .expect("wf-capped record should exist");
    assert_eq!(capped.replay_count, 3);
}

#[test]
#[serial]
fn parallel_init_rebuilds_dispatcher_with_deterministic_capacity() {
    reset_webhook_test_state();

    let mut config = webhook_test_config();
    config.queue_capacity = Some(50);
    config.parallel_queue_multiplier = Some(2.0);

    let standard = current_dispatcher_settings_for_tests(&config);
    assert_eq!(standard, (50, 4));

    init_worker_for_parallel(&config, 5);
    let parallel = current_dispatcher_settings_for_tests(&config);
    assert_eq!(parallel, (500, 5));
}

#[test]
fn webhook_destination_redaction_hides_sensitive_url_components() {
    let redacted = redact_webhook_destination(
        "https://user:secret@example.com/hooks/tenant/token-123?sig=abc#frag",
    );

    assert_eq!(redacted, "https://example.com/…");
    assert!(!redacted.contains("user"));
    assert!(!redacted.contains("secret"));
    assert!(!redacted.contains("token-123"));
    assert!(!redacted.contains("sig=abc"));
}

#[test]
#[serial]
fn failed_delivery_records_store_redacted_destination() {
    reset_webhook_test_state();
    let repo_root = tempfile::tempdir().expect("tempdir");

    let msg = WebhookMessage {
        payload: WebhookPayload {
            event: "task_failed".to_string(),
            timestamp: "2026-02-13T00:00:00Z".to_string(),
            task_id: Some("RQ-0814".to_string()),
            task_title: Some("Secret-safe failure".to_string()),
            previous_status: None,
            current_status: None,
            note: None,
            context: WebhookContext::default(),
        },
        config: ResolvedWebhookConfig {
            enabled: true,
            url: Some("https://user:secret@example.com/hooks/private-token?sig=abc123".to_string()),
            secret: None,
            timeout: Duration::from_secs(1),
            retry_count: 0,
            retry_backoff: Duration::from_millis(10),
        },
    };

    super::diagnostics::persist_failed_delivery_for_tests(
        repo_root.path(),
        &msg,
        &anyhow::anyhow!(
            "delivery to https://user:secret@example.com/hooks/private-token?sig=abc123 failed"
        ),
        1,
    )
    .expect("persist failed delivery");

    let records = super::diagnostics::load_failure_records_for_tests(repo_root.path())
        .expect("load failure records");
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].destination.as_deref(),
        Some("https://example.com/…")
    );
    assert!(!records[0].error.contains("secret"));
    assert!(!records[0].error.contains("private-token"));
    assert!(!records[0].error.contains("sig=abc123"));
}

#[test]
#[serial]
fn retry_backoff_is_scheduled_off_the_hot_worker_path() {
    reset_webhook_test_state();

    let attempts = Arc::new(AtomicUsize::new(0));
    let (events_tx, events_rx) = bounded::<String>(8);
    let attempts_for_transport = Arc::clone(&attempts);
    let events_for_transport = events_tx.clone();

    install_test_transport_for_tests(Some(Arc::new(move |request| {
        let _ = request.body.len();
        let _ = request.signature.clone();
        let _ = request.timeout;

        if request.url.contains("slow.test") {
            let attempt = attempts_for_transport.fetch_add(1, Ordering::SeqCst) + 1;
            events_for_transport
                .send(format!("slow-attempt-{attempt}"))
                .expect("record slow attempt");
            anyhow::bail!("simulated failure");
        }

        events_for_transport
            .send("fast-success".to_string())
            .expect("record fast success");
        Ok(())
    })));

    let slow_config = WebhookConfig {
        url: Some("https://slow.test/hook".to_string()),
        retry_count: Some(2),
        retry_backoff_ms: Some(150),
        ..webhook_test_config()
    };
    let fast_config = WebhookConfig {
        url: Some("https://fast.test/hook".to_string()),
        retry_count: Some(0),
        ..webhook_test_config()
    };

    send_webhook_payload(
        WebhookPayload {
            event: "task_failed".to_string(),
            timestamp: "2026-03-07T00:00:00Z".to_string(),
            task_id: Some("RQ-SLOW".to_string()),
            task_title: Some("Slow endpoint".to_string()),
            previous_status: None,
            current_status: None,
            note: None,
            context: WebhookContext::default(),
        },
        &slow_config,
    );
    send_webhook_payload(
        WebhookPayload {
            event: "task_completed".to_string(),
            timestamp: "2026-03-07T00:00:00Z".to_string(),
            task_id: Some("RQ-FAST".to_string()),
            task_title: Some("Fast endpoint".to_string()),
            previous_status: None,
            current_status: None,
            note: None,
            context: WebhookContext::default(),
        },
        &fast_config,
    );

    let first = events_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first event");
    let second = events_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("second event");
    let third = events_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("third event");

    let first_two = [first.as_str(), second.as_str()];
    assert!(
        first_two.contains(&"slow-attempt-1"),
        "expected initial failure in first two events: {first_two:?}"
    );
    assert!(
        first_two.contains(&"fast-success"),
        "expected unrelated success in first two events: {first_two:?}"
    );
    assert_eq!(third, "slow-attempt-2");
}

#[test]
#[serial]
fn worker_pool_prevents_one_blocked_destination_from_serializing_all_deliveries() {
    reset_webhook_test_state();

    let (blocked_entered_tx, blocked_entered_rx) = bounded::<()>(1);
    let (release_tx, release_rx) = bounded::<()>(1);
    let (events_tx, events_rx) = bounded::<String>(8);

    install_test_transport_for_tests(Some(Arc::new(move |request| {
        if request.url.contains("blocked.test") {
            blocked_entered_tx
                .send(())
                .expect("blocked request entered");
            release_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("release blocked request");
            anyhow::bail!("blocked request released");
        }

        events_tx
            .send("fast-success".to_string())
            .expect("record fast success");
        Ok(())
    })));

    let blocked_config = WebhookConfig {
        url: Some("https://blocked.test/hook".to_string()),
        retry_count: Some(0),
        ..webhook_test_config()
    };
    let fast_config = WebhookConfig {
        url: Some("https://fast.test/hook".to_string()),
        retry_count: Some(0),
        ..webhook_test_config()
    };

    send_webhook_payload(
        WebhookPayload {
            event: "task_failed".to_string(),
            timestamp: "2026-03-07T00:00:00Z".to_string(),
            task_id: Some("RQ-BLOCKED".to_string()),
            task_title: Some("Blocked endpoint".to_string()),
            previous_status: None,
            current_status: None,
            note: None,
            context: WebhookContext::default(),
        },
        &blocked_config,
    );

    blocked_entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("blocked request should start");

    send_webhook_payload(
        WebhookPayload {
            event: "task_completed".to_string(),
            timestamp: "2026-03-07T00:00:00Z".to_string(),
            task_id: Some("RQ-FAST".to_string()),
            task_title: Some("Independent delivery".to_string()),
            previous_status: None,
            current_status: None,
            note: None,
            context: WebhookContext::default(),
        },
        &fast_config,
    );

    let fast_event = events_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("fast delivery should not wait for blocked destination");
    assert_eq!(fast_event, "fast-success");

    release_tx.send(()).expect("release blocked request");
}
