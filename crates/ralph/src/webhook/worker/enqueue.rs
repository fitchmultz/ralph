//! Webhook enqueue and backpressure helpers.
//!
//! Responsibilities:
//! - Apply event filtering and resolved-config gating before enqueue.
//! - Enforce queue backpressure policy at the dispatcher boundary.
//! - Build delivery tasks for both normal dispatch and diagnostics-driven replay.
//!
//! Not handled here:
//! - Dispatcher lifecycle or runtime-mode scaling.
//! - HTTP transport, retries, or signature generation.
//! - Failure-store persistence and replay selection.
//!
//! Invariants/assumptions:
//! - Enqueue remains non-blocking except for the bounded timeout policy.
//! - Queue drop metrics/logs are recorded centrally from the policy branch.
//! - Replay enqueue bypasses event subscription filtering but still respects global enable/url checks.

use crate::contracts::{WebhookConfig, WebhookQueuePolicy};
use crossbeam_channel::{SendTimeoutError, Sender, TrySendError};
use std::time::Duration;

use super::super::diagnostics;
use super::super::types::{ResolvedWebhookConfig, WebhookMessage, WebhookPayload};
use super::runtime::{DeliveryTask, dispatcher_for_config};

/// Apply the configured backpressure policy for a webhook message.
fn apply_backpressure_policy(
    sender: &Sender<DeliveryTask>,
    msg: DeliveryTask,
    policy: WebhookQueuePolicy,
) -> bool {
    let event_type = msg.msg.payload.event.clone();
    let task_id = msg
        .msg
        .payload
        .task_id
        .clone()
        .unwrap_or_else(|| "loop".to_string());

    match policy {
        WebhookQueuePolicy::DropOldest | WebhookQueuePolicy::DropNew => {
            match sender.try_send(msg) {
                Ok(()) => {
                    diagnostics::note_enqueue_success();
                    log::debug!("Webhook enqueued for delivery");
                    true
                }
                Err(TrySendError::Full(_)) => {
                    diagnostics::note_dropped_message();
                    log::warn!(
                        "Webhook queue full; dropping event={} task={}",
                        event_type,
                        task_id
                    );
                    false
                }
                Err(TrySendError::Disconnected(_)) => {
                    diagnostics::note_dropped_message();
                    log::error!(
                        "Webhook dispatcher disconnected; cannot send event={} task={}",
                        event_type,
                        task_id
                    );
                    false
                }
            }
        }
        WebhookQueuePolicy::BlockWithTimeout => {
            match sender.send_timeout(msg, Duration::from_millis(100)) {
                Ok(()) => {
                    diagnostics::note_enqueue_success();
                    log::debug!("Webhook enqueued for delivery");
                    true
                }
                Err(SendTimeoutError::Timeout(_)) => {
                    diagnostics::note_dropped_message();
                    log::warn!(
                        "Webhook queue full (timeout); dropping event={} task={}",
                        event_type,
                        task_id
                    );
                    false
                }
                Err(SendTimeoutError::Disconnected(_)) => {
                    diagnostics::note_dropped_message();
                    log::error!(
                        "Webhook dispatcher disconnected; cannot send event={} task={}",
                        event_type,
                        task_id
                    );
                    false
                }
            }
        }
    }
}

/// Enqueue a webhook payload for replay (internal use).
pub(crate) fn enqueue_webhook_payload_for_replay(
    payload: WebhookPayload,
    config: &WebhookConfig,
) -> bool {
    send_webhook_payload_internal(payload, config, true)
}

/// Internal function to send a webhook payload.
pub(crate) fn send_webhook_payload_internal(
    payload: WebhookPayload,
    config: &WebhookConfig,
    bypass_event_filter: bool,
) -> bool {
    if !bypass_event_filter && !config.is_event_enabled(&payload.event) {
        log::debug!("Webhook for event {} is disabled; skipping", payload.event);
        return false;
    }

    let resolved = ResolvedWebhookConfig::from_config(config);
    if !resolved.enabled {
        log::debug!("Webhooks globally disabled; skipping");
        return false;
    }

    let url = match &resolved.url {
        Some(url) if !url.trim().is_empty() => url.clone(),
        _ => {
            log::debug!("Webhook URL not configured; skipping");
            return false;
        }
    };

    let dispatcher = dispatcher_for_config(config);
    let policy = config.queue_policy.unwrap_or_default();
    let msg = DeliveryTask {
        msg: WebhookMessage {
            payload,
            config: ResolvedWebhookConfig {
                enabled: resolved.enabled,
                url: Some(url),
                secret: resolved.secret,
                timeout: resolved.timeout,
                retry_count: resolved.retry_count,
                retry_backoff: resolved.retry_backoff,
            },
        },
        attempt: 0,
    };

    apply_backpressure_policy(&dispatcher.ready_sender, msg, policy)
}
