//! Webhook notification convenience functions.
//!
//! Purpose:
//! - Webhook notification convenience functions.
//!
//! Responsibilities:
//! - Provide convenience functions for sending common webhook notifications.
//!
//! Not handled here:
//! - Type definitions (see `super::types`).
//! - Worker/delivery logic (see `super::worker`).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::contracts::WebhookConfig;

use super::types::{WebhookContext, WebhookEventType, WebhookPayload};
use super::worker::send_webhook_payload_internal;

/// Send a webhook notification (non-blocking, enqueues for delivery).
///
/// This function returns immediately after enqueueing the webhook.
/// Delivery happens asynchronously in a background worker thread.
#[allow(clippy::too_many_arguments)]
pub fn send_webhook(
    event_type: WebhookEventType,
    task_id: &str,
    task_title: &str,
    previous_status: Option<&str>,
    current_status: Option<&str>,
    note: Option<&str>,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
) {
    let payload = WebhookPayload {
        event: event_type.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: Some(task_id.to_string()),
        task_title: Some(task_title.to_string()),
        previous_status: previous_status.map(|s| s.to_string()),
        current_status: current_status.map(|s| s.to_string()),
        note: note.map(|n| n.to_string()),
        context: WebhookContext::default(),
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Send a webhook payload directly (non-blocking, enqueues for delivery).
///
/// This is the low-level function that checks event filtering and enqueues.
/// Prefer using the `notify_*` convenience functions for common events.
pub fn send_webhook_payload(payload: WebhookPayload, config: &WebhookConfig) {
    if !send_webhook_payload_internal(payload, config, false) {
        // Detailed warning already logged by apply_backpressure_policy
        log::debug!("Webhook enqueue failed (see warning above for details)");
    }
}

/// Convenience function to send task creation webhook.
pub fn notify_task_created(
    task_id: &str,
    task_title: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
) {
    send_webhook(
        WebhookEventType::TaskCreated,
        task_id,
        task_title,
        None,
        None,
        None,
        config,
        timestamp_rfc3339,
    );
}

/// Convenience function to send task creation webhook with context.
pub fn notify_task_created_with_context(
    task_id: &str,
    task_title: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::TaskCreated.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: Some(task_id.to_string()),
        task_title: Some(task_title.to_string()),
        previous_status: None,
        current_status: None,
        note: None,
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Convenience function to send task started webhook.
pub fn notify_task_started(
    task_id: &str,
    task_title: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
) {
    send_webhook(
        WebhookEventType::TaskStarted,
        task_id,
        task_title,
        Some("todo"),
        Some("doing"),
        None,
        config,
        timestamp_rfc3339,
    );
}

/// Convenience function to send task started webhook with context.
pub fn notify_task_started_with_context(
    task_id: &str,
    task_title: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::TaskStarted.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: Some(task_id.to_string()),
        task_title: Some(task_title.to_string()),
        previous_status: Some("todo".to_string()),
        current_status: Some("doing".to_string()),
        note: None,
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Convenience function to send task completed webhook.
pub fn notify_task_completed(
    task_id: &str,
    task_title: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
) {
    send_webhook(
        WebhookEventType::TaskCompleted,
        task_id,
        task_title,
        Some("doing"),
        Some("done"),
        None,
        config,
        timestamp_rfc3339,
    );
}

/// Convenience function to send task completed webhook with context.
pub fn notify_task_completed_with_context(
    task_id: &str,
    task_title: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::TaskCompleted.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: Some(task_id.to_string()),
        task_title: Some(task_title.to_string()),
        previous_status: Some("doing".to_string()),
        current_status: Some("done".to_string()),
        note: None,
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Convenience function to send task failed/rejected webhook.
pub fn notify_task_failed(
    task_id: &str,
    task_title: &str,
    note: Option<&str>,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
) {
    send_webhook(
        WebhookEventType::TaskFailed,
        task_id,
        task_title,
        Some("doing"),
        Some("rejected"),
        note,
        config,
        timestamp_rfc3339,
    );
}

/// Convenience function to send task failed webhook with context.
pub fn notify_task_failed_with_context(
    task_id: &str,
    task_title: &str,
    note: Option<&str>,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::TaskFailed.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: Some(task_id.to_string()),
        task_title: Some(task_title.to_string()),
        previous_status: Some("doing".to_string()),
        current_status: Some("rejected".to_string()),
        note: note.map(|n| n.to_string()),
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Convenience function to send generic status change webhook.
pub fn notify_status_changed(
    task_id: &str,
    task_title: &str,
    previous_status: &str,
    current_status: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
) {
    send_webhook(
        WebhookEventType::TaskStatusChanged,
        task_id,
        task_title,
        Some(previous_status),
        Some(current_status),
        None,
        config,
        timestamp_rfc3339,
    );
}

/// Convenience function to send loop started webhook.
/// This is a loop-level event with no task association.
pub fn notify_loop_started(
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::LoopStarted.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: None,
        task_title: None,
        previous_status: None,
        current_status: None,
        note: None,
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Convenience function to send loop stopped webhook.
/// This is a loop-level event with no task association.
pub fn notify_loop_stopped(
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
    note: Option<&str>,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::LoopStopped.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: None,
        task_title: None,
        previous_status: None,
        current_status: None,
        note: note.map(|n| n.to_string()),
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Convenience function to send phase started webhook.
pub fn notify_phase_started(
    task_id: &str,
    task_title: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::PhaseStarted.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: Some(task_id.to_string()),
        task_title: Some(task_title.to_string()),
        previous_status: None,
        current_status: None,
        note: None,
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Convenience function to send phase completed webhook.
pub fn notify_phase_completed(
    task_id: &str,
    task_title: &str,
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::PhaseCompleted.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: Some(task_id.to_string()),
        task_title: Some(task_title.to_string()),
        previous_status: None,
        current_status: None,
        note: None,
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}

/// Convenience function to send queue unblocked webhook.
/// This is a loop-level event with no task association.
pub fn notify_queue_unblocked(
    config: &WebhookConfig,
    timestamp_rfc3339: &str,
    context: WebhookContext,
    note: Option<&str>,
) {
    let payload = WebhookPayload {
        event: WebhookEventType::QueueUnblocked.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: None,
        task_title: None,
        previous_status: Some("blocked".to_string()),
        current_status: Some("runnable".to_string()),
        note: note.map(|n| n.to_string()),
        context,
    };
    send_webhook_payload_internal(payload, config, false);
}
