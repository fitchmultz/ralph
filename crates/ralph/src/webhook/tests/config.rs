//! Webhook config and payload tests.
//!
//! Purpose:
//! - Webhook config and payload tests.
//!
//! Responsibilities:
//! - Verify event parsing, payload serialization, and config defaults/merging.
//! - Guard compatibility for queue policy parsing and bounded queue settings.
//!
//! Non-scope:
//! - Diagnostics persistence.
//! - Worker pool or retry scheduling behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Serialization tests assert on stable JSON fragments rather than field ordering.

use super::super::*;
use crate::contracts::WebhookConfig;
use crate::webhook::types::{WebhookContext, WebhookPayload};
use crate::webhook::worker::generate_signature;
use std::time::Duration;

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
    assert!(!resolved.allow_insecure_http);
    assert!(!resolved.allow_private_targets);
}

#[test]
fn is_event_enabled_legacy_defaults_only() {
    let config = WebhookConfig {
        enabled: Some(true),
        ..Default::default()
    };

    assert!(config.is_event_enabled("task_created"));
    assert!(config.is_event_enabled("task_started"));
    assert!(config.is_event_enabled("task_completed"));
    assert!(config.is_event_enabled("task_failed"));
    assert!(config.is_event_enabled("task_status_changed"));
    assert!(!config.is_event_enabled("loop_started"));
    assert!(!config.is_event_enabled("loop_stopped"));
    assert!(!config.is_event_enabled("phase_started"));
    assert!(!config.is_event_enabled("phase_completed"));
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

    let config = WebhookConfig {
        enabled: Some(true),
        events: Some(vec![WebhookEventSubscription::Wildcard]),
        ..Default::default()
    };

    assert!(config.is_event_enabled("task_created"));
    assert!(config.is_event_enabled("task_started"));
    assert!(config.is_event_enabled("task_completed"));
    assert!(config.is_event_enabled("loop_started"));
    assert!(config.is_event_enabled("loop_stopped"));
    assert!(config.is_event_enabled("phase_started"));
    assert!(config.is_event_enabled("phase_completed"));
    assert!(config.is_event_enabled("any_event"));
    assert!(config.is_event_enabled("custom_event"));
}

#[test]
fn is_event_enabled_opt_in_new_events() {
    use crate::contracts::WebhookEventSubscription;

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
    assert_eq!(sig.len(), 71);
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
    assert!(!json.contains("previous_status"));
    assert!(!json.contains("runner"));
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
    let config_json = r#"{"queue_policy": "drop_oldest"}"#;
    let config: WebhookConfig = serde_json::from_str(config_json).unwrap();
    assert_eq!(config.queue_policy, Some(WebhookQueuePolicy::DropOldest));

    let config_json = r#"{"queue_policy": "drop_new"}"#;
    let config: WebhookConfig = serde_json::from_str(config_json).unwrap();
    assert_eq!(config.queue_policy, Some(WebhookQueuePolicy::DropNew));

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
fn webhook_config_merge_includes_url_policy_fields() {
    let mut base = WebhookConfig {
        allow_insecure_http: Some(false),
        allow_private_targets: Some(false),
        ..Default::default()
    };
    let other = WebhookConfig {
        allow_insecure_http: Some(true),
        allow_private_targets: Some(true),
        ..Default::default()
    };

    base.merge_from(other);

    assert_eq!(base.allow_insecure_http, Some(true));
    assert_eq!(base.allow_private_targets, Some(true));
}

#[test]
fn webhook_queue_capacity_bounds_check() {
    let low_config = WebhookConfig {
        queue_capacity: Some(0),
        ..Default::default()
    };
    let capacity = low_config
        .queue_capacity
        .map(|value| value.clamp(1u32, 10000))
        .unwrap_or(100);
    assert_eq!(capacity, 1);

    let high_config = WebhookConfig {
        queue_capacity: Some(50000),
        ..Default::default()
    };
    let capacity = high_config
        .queue_capacity
        .map(|value| value.clamp(1u32, 10000))
        .unwrap_or(100);
    assert_eq!(capacity, 10000);

    let normal_config = WebhookConfig {
        queue_capacity: Some(500),
        ..Default::default()
    };
    let capacity = normal_config
        .queue_capacity
        .map(|value| value.clamp(1u32, 10000))
        .unwrap_or(100);
    assert_eq!(capacity, 500);
}
