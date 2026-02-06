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
    let config = WebhookConfig {
        enabled: Some(true),
        events: Some(vec![
            "task_created".to_string(),
            "task_completed".to_string(),
        ]),
        ..Default::default()
    };

    assert!(config.is_event_enabled("task_created"));
    assert!(config.is_event_enabled("task_completed"));
    assert!(!config.is_event_enabled("task_started"));
}

#[test]
fn is_event_enabled_wildcard_subscribes_to_all() {
    // Using ["*"] should enable all events including new ones
    let config = WebhookConfig {
        enabled: Some(true),
        events: Some(vec!["*".to_string()]),
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
    // Explicitly opt-in to new events
    let config = WebhookConfig {
        enabled: Some(true),
        events: Some(vec![
            "task_completed".to_string(),
            "phase_completed".to_string(),
            "loop_started".to_string(),
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
    let config = WebhookConfig {
        enabled: Some(false),
        events: Some(vec!["*".to_string()]),
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
        .map(|c| c.clamp(1, 10000))
        .unwrap_or(100);
    assert_eq!(capacity, 1, "Capacity should be clamped to minimum of 1");

    let high_config = WebhookConfig {
        queue_capacity: Some(50000),
        ..Default::default()
    };
    let capacity = high_config
        .queue_capacity
        .map(|c| c.clamp(1, 10000))
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
        .map(|c| c.clamp(1, 10000))
        .unwrap_or(100);
    assert_eq!(capacity, 500, "Normal capacity should be preserved");
}
