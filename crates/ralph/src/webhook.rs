//! Webhook notification system for task event HTTP callbacks.
//!
//! Responsibilities:
//! - Send HTTP POST requests to configured webhook endpoints.
//! - Generate HMAC-SHA256 signatures for webhook verification.
//! - Retry failed deliveries with exponential backoff.
//! - Provide graceful degradation when webhooks are unavailable.
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
//! - Retry logic respects configured limits and backoff.

use crate::contracts::WebhookConfig;
use serde::Serialize;
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

/// Send a webhook notification.
/// Silently logs errors but never fails the calling operation.
///
/// # Arguments
/// * `event_type` - The type of event being reported
/// * `task_id` - The task identifier
/// * `task_title` - The task title
/// * `previous_status` - Previous status (for status change events)
/// * `current_status` - Current status (for status change events)
/// * `note` - Optional note/context
/// * `config` - Webhook configuration
/// * `timestamp_rfc3339` - Current timestamp in RFC3339 format
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

    // Send with retries
    if let Err(e) = send_with_retry(&url, &payload, &resolved) {
        log::warn!("Webhook delivery failed after retries: {}", e);
    } else {
        log::debug!("Webhook delivered successfully to {}", url);
    }
}

/// Send webhook with retry logic.
fn send_with_retry(
    url: &str,
    payload: &WebhookPayload,
    config: &ResolvedWebhookConfig,
) -> anyhow::Result<()> {
    let body = serde_json::to_string(payload)?;
    let signature = config
        .secret
        .as_ref()
        .map(|secret| generate_signature(&body, secret));

    let mut last_error = None;

    for attempt in 0..=config.retry_count {
        if attempt > 0 {
            let backoff = config.retry_backoff.as_millis() as u64 * attempt as u64;
            std::thread::sleep(Duration::from_millis(backoff));
            log::debug!("Webhook retry attempt {} after {}ms", attempt, backoff);
        }

        match send_request(url, &body, signature.as_deref(), config.timeout) {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::debug!("Webhook attempt {} failed: {}", attempt + 1, e);
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All webhook attempts failed")))
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
}
