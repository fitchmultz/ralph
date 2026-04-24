//! Shared webhook test fixtures.
//!
//! Purpose:
//! - Shared webhook test fixtures.
//!
//! Responsibilities:
//! - Provide reusable sample failure records and default webhook config builders.
//! - Reset global dispatcher/diagnostics state between serial tests.
//!
//! Non-scope:
//! - Assertions for specific webhook behavior.
//! - Delivery transport simulation logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Fixture timestamps remain fixed for deterministic assertions.
//! - Default config keeps retries/timeouts low for unit tests.

use super::super::*;
use crate::contracts::WebhookConfig;
use crate::webhook::types::{WebhookContext, WebhookPayload};
use crate::webhook::worker::reset_dispatcher_for_tests;

pub(super) fn sample_failure_record(
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

pub(super) fn reset_webhook_test_state() {
    crate::webhook::diagnostics::reset_webhook_metrics_for_tests();
    reset_dispatcher_for_tests();
}

pub(super) fn webhook_test_config() -> WebhookConfig {
    use crate::contracts::WebhookEventSubscription;

    WebhookConfig {
        enabled: Some(true),
        url: Some("http://127.0.0.1:9/webhook".to_string()),
        allow_insecure_http: Some(true),
        allow_private_targets: Some(true),
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
