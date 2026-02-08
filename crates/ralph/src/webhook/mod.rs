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
//! - UI mode detection (callers should suppress if desired).
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
    /// Run loop started.
    LoopStarted,
    /// Run loop stopped (success, failure, or signal).
    LoopStopped,
    /// Phase started for a task.
    PhaseStarted,
    /// Phase completed for a task.
    PhaseCompleted,
    /// Queue became unblocked (runnable tasks available after being blocked).
    QueueUnblocked,
}

impl WebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WebhookEventType::TaskCreated => "task_created",
            WebhookEventType::TaskStarted => "task_started",
            WebhookEventType::TaskCompleted => "task_completed",
            WebhookEventType::TaskFailed => "task_failed",
            WebhookEventType::TaskStatusChanged => "task_status_changed",
            WebhookEventType::LoopStarted => "loop_started",
            WebhookEventType::LoopStopped => "loop_stopped",
            WebhookEventType::PhaseStarted => "phase_started",
            WebhookEventType::PhaseCompleted => "phase_completed",
            WebhookEventType::QueueUnblocked => "queue_unblocked",
        }
    }
}

impl std::str::FromStr for WebhookEventType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "task_created" => Self::TaskCreated,
            "task_started" => Self::TaskStarted,
            "task_completed" => Self::TaskCompleted,
            "task_failed" => Self::TaskFailed,
            "task_status_changed" => Self::TaskStatusChanged,
            "loop_started" => Self::LoopStarted,
            "loop_stopped" => Self::LoopStopped,
            "phase_started" => Self::PhaseStarted,
            "phase_completed" => Self::PhaseCompleted,
            "queue_unblocked" => Self::QueueUnblocked,
            other => anyhow::bail!(
                "Unknown event type: {}. Supported: task_created, task_started, task_completed, task_failed, task_status_changed, loop_started, loop_stopped, phase_started, phase_completed, queue_unblocked",
                other
            ),
        })
    }
}

/// Optional context metadata for webhook payloads.
/// These fields are only serialized when set (Some).
#[derive(Debug, Clone, Default, Serialize)]
pub struct WebhookContext {
    /// Runner used for this phase/execution (e.g., "claude", "codex").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    /// Model used for this phase/execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Current phase number (1, 2, or 3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<u8>,
    /// Total number of phases configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_count: Option<u8>,
    /// Duration in milliseconds (for completed operations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Repository root path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    /// Current git branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Current git commit hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// CI gate outcome: "skipped", "passed", or "failed".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_gate: Option<String>,
}

/// Webhook event payload structure.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    /// Event type identifier.
    pub event: String,
    /// Timestamp of the event (RFC3339).
    pub timestamp: String,
    /// Task ID (e.g., "RQ-0001").
    /// Optional: may be None for loop-level events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Task title.
    /// Optional: may be None for loop-level events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    /// Previous status (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_status: Option<String>,
    /// Current/new status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_status: Option<String>,
    /// Additional context or notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Optional context metadata (runner, model, phase, git info, etc.)
    #[serde(flatten)]
    pub context: WebhookContext,
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

/// Send a webhook payload directly (non-blocking, enqueues for delivery).
///
/// This is the low-level function that checks event filtering and enqueues.
/// Prefer using the `notify_*` convenience functions for common events.
pub fn send_webhook_payload(payload: WebhookPayload, config: &WebhookConfig) {
    // Check if webhooks are enabled for this event type
    if !config.is_event_enabled(&payload.event) {
        log::debug!("Webhook for event {} is disabled; skipping", payload.event);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
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
    send_webhook_payload(payload, config);
}

#[cfg(test)]
mod tests;
