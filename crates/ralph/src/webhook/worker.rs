//! Webhook worker and delivery logic.
//!
//! Responsibilities:
//! - Manage the background worker thread for webhook delivery.
//! - Handle HTTP delivery with retries and HMAC signatures.
//! - Apply backpressure policies for the webhook queue.
//!
//! Not handled here:
//! - Type definitions (see `super::types`).
//! - Notification convenience functions (see `super::notifications`).
//! - Diagnostics and replay (see `super::diagnostics`).

use crate::contracts::{WebhookConfig, WebhookQueuePolicy};
use crossbeam_channel::{Sender, TrySendError, bounded};
use std::sync::OnceLock;
use std::time::Duration;

use super::diagnostics;
use super::types::{ResolvedWebhookConfig, WebhookMessage, WebhookPayload};

/// Global webhook channel pair for backpressure handling.
/// This is stored in a OnceLock and initialized on first use.
struct WebhookChannel {
    sender: Sender<WebhookMessage>,
    // Note: receiver is moved into the worker thread, not stored here
}

// Global channel - initialized on first use.
static CHANNEL: OnceLock<WebhookChannel> = OnceLock::new();

/// Initialize the global webhook worker and channel.
pub(crate) fn init_worker(config: &WebhookConfig) {
    // Clamp capacity to valid range (1-10000) to avoid rendezvous channel behavior at 0
    // Default to 500 for better parallel mode handling (was 100, too small for burst loads)
    let capacity = config
        .queue_capacity
        .map(|c| c.clamp(1, 10000))
        .unwrap_or(500) as usize;

    // Use get_or_init to ensure thread-safe one-time initialization
    let _ = CHANNEL.get_or_init(|| {
        let (sender, receiver) = bounded(capacity);
        diagnostics::set_queue_capacity(capacity);

        // Spawn the worker thread (moves receiver into the closure)
        std::thread::spawn(move || {
            log::debug!("Webhook worker started (capacity: {})", capacity);

            while let Ok(msg) = receiver.recv() {
                diagnostics::note_queue_dequeue();
                if let Err(e) = deliver_webhook(&msg) {
                    log::warn!("Webhook delivery failed: {}", e);
                }
            }

            log::debug!("Webhook worker shutting down");
        });

        WebhookChannel {
            sender: sender.clone(),
        }
    });
}

/// Initialize the global webhook worker with capacity scaled for parallel execution.
/// Call this instead of relying on implicit init when running in parallel mode.
///
/// The effective capacity is calculated as:
///   base_capacity * max(1, worker_count * parallel_queue_multiplier)
///
/// This provides a larger queue buffer for parallel mode where multiple workers
/// may send webhooks concurrently while the delivery thread is blocked on slow endpoints.
pub fn init_worker_for_parallel(config: &WebhookConfig, worker_count: u8) {
    let base_capacity = config
        .queue_capacity
        .map(|c| c.clamp(1, 10000))
        .unwrap_or(500) as usize;

    let multiplier = config
        .parallel_queue_multiplier
        .unwrap_or(2.0)
        .clamp(1.0, 10.0);

    // Scale capacity: base * max(1, workers * multiplier), clamped to max
    let scaled =
        (base_capacity as f64 * (worker_count as f64 * multiplier as f64).max(1.0)) as usize;
    let capacity = scaled.clamp(1, 10000);

    // Use get_or_init to ensure thread-safe one-time initialization
    let _ = CHANNEL.get_or_init(|| {
        let (sender, receiver) = bounded(capacity);
        diagnostics::set_queue_capacity(capacity);

        // Spawn the worker thread (moves receiver into the closure)
        std::thread::spawn(move || {
            log::debug!(
                "Webhook worker started (capacity: {}, parallel-optimized for {} workers)",
                capacity,
                worker_count
            );

            while let Ok(msg) = receiver.recv() {
                diagnostics::note_queue_dequeue();
                if let Err(e) = deliver_webhook(&msg) {
                    log::warn!("Webhook delivery failed: {}", e);
                }
            }

            log::debug!("Webhook worker shutting down");
        });

        WebhookChannel {
            sender: sender.clone(),
        }
    });
}

/// Get the global webhook sender.
pub(crate) fn get_sender() -> Option<Sender<WebhookMessage>> {
    CHANNEL.get().map(|ch| ch.sender.clone())
}

/// Deliver a webhook in the worker thread (blocking, with retries).
fn deliver_webhook(msg: &WebhookMessage) -> anyhow::Result<()> {
    let url = msg
        .config
        .url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Webhook URL not configured"))?;

    let body = serde_json::to_string(&msg.payload)?;
    let signature = msg
        .config
        .secret
        .as_ref()
        .map(|secret| generate_signature(&body, secret));

    let mut last_error = None;

    for attempt in 0..=msg.config.retry_count {
        if attempt > 0 {
            diagnostics::note_retry_attempt();
            let backoff = msg.config.retry_backoff.as_millis() as u64 * attempt as u64;
            std::thread::sleep(Duration::from_millis(backoff));
            log::debug!("Webhook retry attempt {} after {}ms", attempt, backoff);
        }

        match send_request(url, &body, signature.as_deref(), msg.config.timeout) {
            Ok(()) => {
                diagnostics::note_delivery_success();
                log::debug!("Webhook delivered successfully to {}", url);
                return Ok(());
            }
            Err(e) => {
                log::debug!("Webhook attempt {} failed: {}", attempt + 1, e);
                last_error = Some(e);
            }
        }
    }

    let final_error = last_error.unwrap_or_else(|| anyhow::anyhow!("All webhook attempts failed"));
    diagnostics::note_delivery_failure(msg, &final_error, msg.config.retry_count.saturating_add(1));
    Err(final_error)
}

/// Send a single HTTP POST request.
fn send_request(
    url: &str,
    body: &str,
    signature: Option<&str>,
    timeout: Duration,
) -> anyhow::Result<()> {
    // In ureq 3.x, we use an Agent with timeout configuration
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build(),
    );

    let mut request = agent
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", concat!("ralph/", env!("CARGO_PKG_VERSION")));

    if let Some(sig) = signature {
        request = request.header("X-Ralph-Signature", sig);
    }

    let response = request.send(body)?;

    let status = response.status();

    if status.is_success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "HTTP {}: webhook endpoint returned error",
            status
        ))
    }
}

/// Generate HMAC-SHA256 signature for webhook payload.
pub(crate) fn generate_signature(body: &str, secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(body.as_bytes());
    let result = mac.finalize();
    let code_bytes = result.into_bytes();

    format!("sha256={}", hex::encode(code_bytes))
}

/// Apply the configured backpressure policy for a webhook message.
pub(crate) fn apply_backpressure_policy(
    sender: &Sender<WebhookMessage>,
    msg: WebhookMessage,
    policy: WebhookQueuePolicy,
) -> bool {
    // Clone event details before moving msg into try_send/send_timeout
    let event_type = msg.payload.event.clone();
    let task_id = msg
        .payload
        .task_id
        .clone()
        .unwrap_or_else(|| "loop".to_string());

    match policy {
        WebhookQueuePolicy::DropOldest => {
            // Drop new webhooks when queue is full, preserving existing queue contents.
            // This is functionally equivalent to `drop_new` due to channel constraints
            // (we cannot pop from the front of the queue from the sender side).
            match sender.try_send(msg) {
                Ok(()) => {
                    diagnostics::note_enqueue_success();
                    log::debug!("Webhook enqueued for delivery");
                    true
                }
                Err(TrySendError::Full(_)) => {
                    // Queue is full - drop the new message
                    diagnostics::note_dropped_message();
                    log::warn!(
                        "Webhook queue full (drop_oldest policy); dropping event={} task={}",
                        event_type,
                        task_id
                    );
                    false
                }
                Err(TrySendError::Disconnected(_)) => {
                    diagnostics::note_dropped_message();
                    log::error!(
                        "Webhook worker disconnected; cannot send event={} task={}",
                        event_type,
                        task_id
                    );
                    false
                }
            }
        }
        WebhookQueuePolicy::DropNew => match sender.try_send(msg) {
            Ok(()) => {
                diagnostics::note_enqueue_success();
                log::debug!("Webhook enqueued for delivery");
                true
            }
            Err(e) => {
                diagnostics::note_dropped_message();
                log::warn!(
                    "Webhook queue full; dropping event={} task={}: {}",
                    event_type,
                    task_id,
                    e
                );
                false
            }
        },
        WebhookQueuePolicy::BlockWithTimeout => {
            // Block briefly (100ms), then drop if still full
            match sender.send_timeout(msg, Duration::from_millis(100)) {
                Ok(()) => {
                    diagnostics::note_enqueue_success();
                    log::debug!("Webhook enqueued for delivery");
                    true
                }
                Err(crossbeam_channel::SendTimeoutError::Timeout(_msg)) => {
                    diagnostics::note_dropped_message();
                    log::warn!(
                        "Webhook queue full (timeout); dropping event={} task={}",
                        event_type,
                        task_id
                    );
                    false
                }
                Err(crossbeam_channel::SendTimeoutError::Disconnected(_)) => {
                    diagnostics::note_dropped_message();
                    log::error!(
                        "Webhook worker disconnected; cannot send event={} task={}",
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

/// Internal function to send webhook payload.
pub(crate) fn send_webhook_payload_internal(
    payload: WebhookPayload,
    config: &WebhookConfig,
    bypass_event_filter: bool,
) -> bool {
    // Check if webhooks are enabled for this event type
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
        Some(url) if !url.is_empty() => url.clone(),
        _ => {
            log::debug!("Webhook URL not configured; skipping");
            return false;
        }
    };

    // Initialize worker on first use
    init_worker(config);

    let policy = config.queue_policy.unwrap_or_default();

    let msg = WebhookMessage {
        payload,
        config: ResolvedWebhookConfig {
            enabled: resolved.enabled,
            url: Some(url),
            secret: resolved.secret,
            timeout: resolved.timeout,
            retry_count: resolved.retry_count,
            retry_backoff: resolved.retry_backoff,
        },
    };

    // Apply backpressure policy
    match get_sender() {
        Some(sender) => apply_backpressure_policy(&sender, msg, policy),
        None => {
            log::error!("Webhook worker not initialized; cannot send webhook");
            false
        }
    }
}
