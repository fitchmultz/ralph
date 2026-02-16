//! Webhook configuration for HTTP task event notifications.
//!
//! Responsibilities:
//! - Define webhook config structs and backpressure policy enum.
//! - Provide merge behavior and event filtering.
//!
//! Not handled here:
//! - Actual webhook delivery (see `crate::webhook` module).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Backpressure policy for webhook delivery queue.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookQueuePolicy {
    /// Drop new webhooks when queue is full, preserving existing queue contents.
    /// This is functionally equivalent to `drop_new` due to channel constraints.
    #[default]
    DropOldest,
    /// Drop the new webhook if queue is full.
    DropNew,
    /// Block sender briefly, then drop if queue is still full.
    BlockWithTimeout,
}

/// Webhook configuration for HTTP task event notifications.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WebhookConfig {
    /// Enable webhook notifications (default: false).
    pub enabled: Option<bool>,

    /// Webhook endpoint URL (required when enabled).
    pub url: Option<String>,

    /// Secret key for HMAC-SHA256 signature generation.
    /// When set, webhooks include an X-Ralph-Signature header.
    pub secret: Option<String>,

    /// Events to subscribe to (default: legacy task events only).
    /// Supported: task_created, task_started, task_completed, task_failed, task_status_changed,
    ///            loop_started, loop_stopped, phase_started, phase_completed
    /// Note: loop_* and phase_* events are opt-in and require explicit configuration.
    /// Use ["*"] to subscribe to all events.
    pub events: Option<Vec<String>>,

    /// Request timeout in seconds (default: 30, max: 300).
    #[schemars(range(min = 1, max = 300))]
    pub timeout_secs: Option<u32>,

    /// Number of retry attempts for failed deliveries (default: 3, max: 10).
    #[schemars(range(min = 0, max = 10))]
    pub retry_count: Option<u32>,

    /// Retry backoff base in milliseconds (default: 1000, max: 30000).
    #[schemars(range(min = 100, max = 30000))]
    pub retry_backoff_ms: Option<u32>,

    /// Maximum number of pending webhooks in the delivery queue (default: 100, range: 10-10000).
    #[schemars(range(min = 10, max = 10000))]
    pub queue_capacity: Option<u32>,

    /// Backpressure policy when queue is full (default: drop_oldest).
    /// - drop_oldest: Drop new webhooks when full (preserves existing queue contents)
    /// - drop_new: Drop the new webhook if queue is full
    /// - block_with_timeout: Block sender briefly (100ms), then drop if still full
    pub queue_policy: Option<WebhookQueuePolicy>,
}

impl WebhookConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.enabled.is_some() {
            self.enabled = other.enabled;
        }
        if other.url.is_some() {
            self.url = other.url;
        }
        if other.secret.is_some() {
            self.secret = other.secret;
        }
        if other.events.is_some() {
            self.events = other.events;
        }
        if other.timeout_secs.is_some() {
            self.timeout_secs = other.timeout_secs;
        }
        if other.retry_count.is_some() {
            self.retry_count = other.retry_count;
        }
        if other.retry_backoff_ms.is_some() {
            self.retry_backoff_ms = other.retry_backoff_ms;
        }
        if other.queue_capacity.is_some() {
            self.queue_capacity = other.queue_capacity;
        }
        if other.queue_policy.is_some() {
            self.queue_policy = other.queue_policy;
        }
    }

    /// Legacy default events that are enabled when `events` is not specified.
    /// New events (loop_*, phase_*) are opt-in and require explicit configuration.
    const DEFAULT_EVENTS_V1: [&'static str; 5] = [
        "task_created",
        "task_started",
        "task_completed",
        "task_failed",
        "task_status_changed",
    ];

    /// Check if a specific event type is enabled.
    ///
    /// Event filtering behavior:
    /// - If webhooks are disabled, no events are sent.
    /// - If `events` is `None`: only legacy task events are enabled (backward compatible).
    /// - If `events` is `Some([...])`: only those events are enabled; use `["*"]` to enable all.
    pub fn is_event_enabled(&self, event: &str) -> bool {
        if !self.enabled.unwrap_or(false) {
            return false;
        }
        match &self.events {
            None => Self::DEFAULT_EVENTS_V1.contains(&event),
            Some(events) => events.iter().any(|e| e == event || e == "*"),
        }
    }
}
