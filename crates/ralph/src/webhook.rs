//! Asynchronous webhook notification system with bounded queue.
//!
//! Responsibilities:
//! - Enqueue webhook events to a background worker (non-blocking).
//! - Background worker handles HTTP delivery with retries.
//! - Bounded queue with configurable backpressure policy.
//! - Generate HMAC-SHA256 signatures for webhook verification.
//!
//! Does NOT handle:
//! - Webhook endpoint management or registration.
//! - Persistent delivery history or logging.
//! - TUI mode detection (callers should suppress if desired).
//! - Response processing beyond HTTP status check.
//!
//! Invariants:
//! - Webhook failures are logged but never fail the calling operation.
//! - Secrets are never logged or exposed in error messages.
//! - All requests include a timeout to prevent hanging.
//! - Queue backpressure protects interactive UX from slow endpoints.
//! - Worker thread is automatically cleaned up on drop.

use crate::contracts::{WebhookConfig, WebhookQueuePolicy};
use crossbeam_channel::{Sender, TrySendError, bounded};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;

/// Types of webhook events that can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookEventType {
    /// Task was created/added to queue.
    TaskCreated,
    /// Task status changed to Doing (execution started).
    TaskStarted,
    /// Task completed successfully (status Done).
    TaskCompleted,
    /// Task failed or was rejected.
    TaskFailed,
    /// Generic status change (used when specific type not applicable).
    TaskStatusChanged,
}

impl WebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WebhookEventType::TaskCreated => "task_created",
            WebhookEventType::TaskStarted => "task_started",
            WebhookEventType::TaskCompleted => "task_completed",
            WebhookEventType::TaskFailed => "task_failed",
            WebhookEventType::TaskStatusChanged => "task_status_changed",
        }
    }
}

/// Webhook event payload structure.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    /// Event type identifier.
    pub event: String,
    /// Timestamp of the event (RFC3339).
    pub timestamp: String,
    /// Task ID (e.g., "RQ-0001").
    pub task_id: String,
    /// Task title.
    pub task_title: String,
    /// Previous status (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_status: Option<String>,
    /// Current/new status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_status: Option<String>,
    /// Additional context or notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Resolved webhook configuration with defaults applied.
#[derive(Debug, Clone)]
pub struct ResolvedWebhookConfig {
    pub enabled: bool,
    pub url: Option<String>,
    pub secret: Option<String>,
    pub timeout: Duration,
    pub retry_count: u32,
    pub retry_backoff: Duration,
}

impl ResolvedWebhookConfig {
    /// Resolve a WebhookConfig to concrete values with defaults.
    pub fn from_config(config: &WebhookConfig) -> Self {
        Self {
            enabled: config.enabled.unwrap_or(false),
            url: config.url.clone(),
            secret: config.secret.clone(),
            timeout: Duration::from_secs(config.timeout_secs.unwrap_or(30) as u64),
            retry_count: config.retry_count.unwrap_or(3),
            retry_backoff: Duration::from_millis(config.retry_backoff_ms.unwrap_or(1000) as u64),
        }
    }
}

/// Internal message for the webhook worker.
#[derive(Debug, Clone)]
struct WebhookMessage {
    payload: WebhookPayload,
    config: ResolvedWebhookConfig,
}

/// Global webhook channel pair for backpressure handling.
/// This is stored in a OnceLock and initialized on first use.
struct WebhookChannel {
    sender: Sender<WebhookMessage>,
    // Note: receiver is moved into the worker thread, not stored here
}

// Global channel - initialized on first use.
static CHANNEL: OnceLock<WebhookChannel> = OnceLock::new();

/// Initialize the global webhook worker and channel.
fn init_worker(config: &WebhookConfig) {
    // Clamp capacity to valid range (1-10000) to avoid rendezvous channel behavior at 0
    let capacity = config
        .queue_capacity
        .map(|c| c.clamp(1, 10000))
        .unwrap_or(100) as usize;

    // Use get_or_init to ensure thread-safe one-time initialization
    let _ = CHANNEL.get_or_init(|| {
        let (sender, receiver) = bounded(capacity);

        // Spawn the worker thread (moves receiver into the closure)
        std::thread::spawn(move || {
            log::debug!("Webhook worker started (capacity: {})", capacity);

            while let Ok(msg) = receiver.recv() {
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
fn get_sender() -> Option<Sender<WebhookMessage>> {
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
            let backoff = msg.config.retry_backoff.as_millis() as u64 * attempt as u64;
            std::thread::sleep(Duration::from_millis(backoff));
            log::debug!("Webhook retry attempt {} after {}ms", attempt, backoff);
        }

        match send_request(url, &body, signature.as_deref(), msg.config.timeout) {
            Ok(()) => {
                log::debug!("Webhook delivered successfully to {}", url);
                return Ok(());
            }
            Err(e) => {
                log::debug!("Webhook attempt {} failed: {}", attempt + 1, e);
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All webhook attempts failed")))
}

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
    // Check if webhooks are enabled for this event type
    if !config.is_event_enabled(event_type.as_str()) {
        log::debug!(
            "Webhook for event {} is disabled; skipping",
            event_type.as_str()
        );
        return;
    }

    let resolved = ResolvedWebhookConfig::from_config(config);

    if !resolved.enabled {
        log::debug!("Webhooks globally disabled; skipping");
        return;
    }

    let url = match &resolved.url {
        Some(url) if !url.is_empty() => url.clone(),
        _ => {
            log::debug!("Webhook URL not configured; skipping");
            return;
        }
    };

    // Build payload
    let payload = WebhookPayload {
        event: event_type.as_str().to_string(),
        timestamp: timestamp_rfc3339.to_string(),
        task_id: task_id.to_string(),
        task_title: task_title.to_string(),
        previous_status: previous_status.map(|s| s.to_string()),
        current_status: current_status.map(|s| s.to_string()),
        note: note.map(|n| n.to_string()),
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
        }
    }
}

/// Apply the configured backpressure policy for a webhook message.
fn apply_backpressure_policy(
    sender: &Sender<WebhookMessage>,
    msg: WebhookMessage,
    policy: WebhookQueuePolicy,
) {
    match policy {
        WebhookQueuePolicy::DropOldest => {
            // Drop new webhooks when queue is full, preserving existing queue contents.
            // This is functionally equivalent to `drop_new` due to channel constraints
            // (we cannot pop from the front of the queue from the sender side).
            match sender.try_send(msg) {
                Ok(()) => {
                    log::debug!("Webhook enqueued for delivery");
                }
                Err(TrySendError::Full(_)) => {
                    // Queue is full - drop the new message
                    log::warn!("Webhook queue full (drop_oldest policy); dropping new message");
                }
                Err(TrySendError::Disconnected(_)) => {
                    log::error!("Webhook worker disconnected; cannot send webhook");
                }
            }
        }
        WebhookQueuePolicy::DropNew => {
            if let Err(e) = sender.try_send(msg) {
                log::warn!("Webhook queue full; dropping message: {}", e);
            } else {
                log::debug!("Webhook enqueued for delivery");
            }
        }
        WebhookQueuePolicy::BlockWithTimeout => {
            // Block briefly (100ms), then drop if still full
            match sender.send_timeout(msg, Duration::from_millis(100)) {
                Ok(()) => {
                    log::debug!("Webhook enqueued for delivery");
                }
                Err(crossbeam_channel::SendTimeoutError::Timeout(_msg)) => {
                    log::warn!("Webhook queue full (timeout); dropping message");
                }
                Err(crossbeam_channel::SendTimeoutError::Disconnected(_)) => {
                    log::error!("Webhook worker disconnected; cannot send webhook");
                }
            }
        }
    }
}

/// Send a single HTTP POST request.
fn send_request(
    url: &str,
    body: &str,
    signature: Option<&str>,
    timeout: Duration,
) -> anyhow::Result<()> {
    let mut request = ureq::post(url)
        .set("Content-Type", "application/json")
        .set("User-Agent", concat!("ralph/", env!("CARGO_PKG_VERSION")));

    if let Some(sig) = signature {
        request = request.set("X-Ralph-Signature", sig);
    }

    let response = request.timeout(timeout).send_string(body)?;

    let status = response.status();

    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "HTTP {}: webhook endpoint returned error",
            status
        ))
    }
}

/// Generate HMAC-SHA256 signature for webhook payload.
fn generate_signature(body: &str, secret: &str) -> String {
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

#[cfg(test)]
mod tests {
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
    fn is_event_enabled_all_by_default() {
        let config = WebhookConfig {
            enabled: Some(true),
            ..Default::default()
        };

        assert!(config.is_event_enabled("task_created"));
        assert!(config.is_event_enabled("task_completed"));
        assert!(config.is_event_enabled("any_event"));
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
            task_id: "RQ-0001".to_string(),
            task_title: "Test task".to_string(),
            previous_status: None,
            current_status: Some("todo".to_string()),
            note: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"event\":\"task_created\""));
        assert!(json.contains("\"task_id\":\"RQ-0001\""));
        assert!(!json.contains("previous_status")); // skipped when None
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
}
