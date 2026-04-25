//! Webhook configuration for HTTP task event notifications.
//!
//! Purpose:
//! - Webhook configuration for HTTP task event notifications.
//!
//! Responsibilities:
//! - Define webhook config structs and backpressure policy enum.
//! - Provide merge behavior and event filtering.
//! - Define valid webhook event subscription types for config validation.
//!
//! Not handled here:
//! - Actual webhook delivery (see `crate::webhook` module).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::{Context, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::{Host, Url};

/// Webhook event subscription type for config.
/// Each variant corresponds to a WebhookEventType, plus Wildcard for "all events".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventSubscription {
    /// Task was created/added to queue.
    TaskCreated,
    /// Task status changed to Doing (execution started).
    TaskStarted,
    /// Task completed successfully (status Done).
    TaskCompleted,
    /// Task failed or was rejected.
    TaskFailed,
    /// Generic status change.
    TaskStatusChanged,
    /// Run loop started.
    LoopStarted,
    /// Run loop stopped.
    LoopStopped,
    /// Phase started for a task.
    PhaseStarted,
    /// Phase completed for a task.
    PhaseCompleted,
    /// Queue became unblocked.
    QueueUnblocked,
    /// Wildcard: subscribe to all events.
    #[serde(rename = "*")]
    Wildcard,
}

impl WebhookEventSubscription {
    /// Convert to the string representation used in event matching.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TaskCreated => "task_created",
            Self::TaskStarted => "task_started",
            Self::TaskCompleted => "task_completed",
            Self::TaskFailed => "task_failed",
            Self::TaskStatusChanged => "task_status_changed",
            Self::LoopStarted => "loop_started",
            Self::LoopStopped => "loop_stopped",
            Self::PhaseStarted => "phase_started",
            Self::PhaseCompleted => "phase_completed",
            Self::QueueUnblocked => "queue_unblocked",
            Self::Wildcard => "*",
        }
    }
}

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

    /// When `true`, allow `http://` webhook URLs. Default is HTTPS-only (`false` / unset).
    #[schemars(
        description = "Opt-in to allow plaintext http:// webhook URLs (default: HTTPS only)."
    )]
    pub allow_insecure_http: Option<bool>,

    /// When `true`, allow loopback, link-local, and common cloud metadata hostnames/IPs.
    /// Default blocks these SSRF-adjacent targets unless explicitly opted in.
    #[schemars(
        description = "Opt-in to allow loopback, link-local (169.254/…), and metadata-style hosts."
    )]
    pub allow_private_targets: Option<bool>,

    /// Secret key for HMAC-SHA256 signature generation.
    /// When set, webhooks include an X-Ralph-Signature header.
    pub secret: Option<String>,

    /// Events to subscribe to (default: legacy task events only).
    pub events: Option<Vec<WebhookEventSubscription>>,

    /// Request timeout in seconds (default: 30, max: 300).
    #[schemars(range(min = 1, max = 300))]
    pub timeout_secs: Option<u32>,

    /// Number of retry attempts for failed deliveries (default: 3, max: 10).
    #[schemars(range(min = 0, max = 10))]
    pub retry_count: Option<u32>,

    /// Base interval for exponential webhook retry delays in milliseconds (default: 1000, max: 30000).
    /// Actual delays apply bounded jitter and cap at 30 seconds between attempts.
    #[schemars(range(min = 100, max = 30000))]
    pub retry_backoff_ms: Option<u32>,

    /// Maximum number of pending webhooks in the delivery queue (default: 500, range: 10-10000).
    #[schemars(range(min = 10, max = 10000))]
    pub queue_capacity: Option<u32>,

    /// Multiplier for queue capacity in parallel mode (default: 2.0, range: 1.0-10.0).
    /// When running with N parallel workers, effective capacity = queue_capacity * max(1, workers * multiplier).
    /// Set higher (e.g., 3.0) if webhook endpoint is slow or unreliable.
    #[schemars(range(min = 1.0, max = 10.0))]
    pub parallel_queue_multiplier: Option<f32>,

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
        if other.allow_insecure_http.is_some() {
            self.allow_insecure_http = other.allow_insecure_http;
        }
        if other.allow_private_targets.is_some() {
            self.allow_private_targets = other.allow_private_targets;
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
        if other.parallel_queue_multiplier.is_some() {
            self.parallel_queue_multiplier = other.parallel_queue_multiplier;
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
            Some(events) => events
                .iter()
                .any(|e| e.as_str() == event || e.as_str() == "*"),
        }
    }
}

/// Validate webhook URL when `agent.webhook.enabled` is true (requires non-empty URL and safety rules).
pub(crate) fn validate_webhook_settings(cfg: &WebhookConfig) -> anyhow::Result<()> {
    validate_u32_range("agent.webhook.timeout_secs", cfg.timeout_secs, 1, 300)?;
    validate_u32_range("agent.webhook.retry_count", cfg.retry_count, 0, 10)?;
    validate_u32_range(
        "agent.webhook.retry_backoff_ms",
        cfg.retry_backoff_ms,
        100,
        30_000,
    )?;
    validate_u32_range(
        "agent.webhook.queue_capacity",
        cfg.queue_capacity,
        10,
        10_000,
    )?;
    validate_f32_range(
        "agent.webhook.parallel_queue_multiplier",
        cfg.parallel_queue_multiplier,
        1.0,
        10.0,
    )?;

    if !cfg.enabled.unwrap_or(false) {
        return Ok(());
    }
    let Some(raw) = cfg.url.as_deref() else {
        bail!(
            "agent.webhook.enabled=true requires agent.webhook.url to be set to an absolute https:// URL"
        );
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("agent.webhook.enabled=true requires a non-empty agent.webhook.url");
    }
    validate_webhook_destination_url(
        trimmed,
        cfg.allow_insecure_http.unwrap_or(false),
        cfg.allow_private_targets.unwrap_or(false),
    )
}

fn validate_u32_range(field: &str, value: Option<u32>, min: u32, max: u32) -> anyhow::Result<()> {
    if let Some(value) = value
        && !(min..=max).contains(&value)
    {
        bail!("Invalid {field}: {value}. Expected a value between {min} and {max}.");
    }
    Ok(())
}

fn validate_f32_range(field: &str, value: Option<f32>, min: f32, max: f32) -> anyhow::Result<()> {
    if let Some(value) = value
        && (!(min..=max).contains(&value) || !value.is_finite())
    {
        bail!("Invalid {field}: {value}. Expected a value between {min} and {max}.");
    }
    Ok(())
}

/// Validate a webhook destination URL for delivery or config-time checks.
///
/// - Only `http` and `https` schemes are accepted.
/// - `http` requires `allow_insecure_http`.
/// - Loopback, IPv4 link-local (`169.254.0.0/16`), IPv6 link-local, and common metadata hostnames
///   are rejected unless `allow_private_targets` is true.
pub(crate) fn validate_webhook_destination_url(
    raw_url: &str,
    allow_insecure_http: bool,
    allow_private_targets: bool,
) -> anyhow::Result<()> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        bail!("webhook URL is empty");
    }

    let parsed = Url::parse(trimmed).context("webhook URL must be a valid absolute URL")?;

    match parsed.scheme() {
        "https" => {}
        "http" => {
            if !allow_insecure_http {
                bail!(
                    "webhook URL uses http://; only https:// is allowed by default. \
                     Set agent.webhook.allow_insecure_http=true to permit plaintext HTTP (not recommended)."
                );
            }
        }
        other => {
            bail!(
                "webhook URL scheme {other:?} is not allowed; only http:// and https:// are supported"
            );
        }
    }

    if parsed.host_str().is_none_or(|h| h.is_empty()) {
        bail!("webhook URL must include a non-empty host");
    }

    if !allow_private_targets && url_host_is_ssrf_risk(&parsed) {
        bail!(
            "webhook URL targets a loopback, link-local, or cloud-metadata-style host, which is blocked by default. \
             Set agent.webhook.allow_private_targets=true only if you intentionally send webhooks to such a destination."
        );
    }

    Ok(())
}

fn url_host_is_ssrf_risk(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(ip)) => ip_is_blocked_private_adjacent(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => ip_is_blocked_private_adjacent(IpAddr::V6(ip)),
        Some(Host::Domain(domain)) => domain_host_is_risky(domain),
        None => true,
    }
}

fn ip_is_blocked_private_adjacent(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => ipv4_is_risky(v4),
        IpAddr::V6(v6) => ipv6_is_risky(v6),
    }
}

fn ipv4_is_risky(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
}

fn ipv6_is_risky(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return ipv4_is_risky(mapped);
    }
    ip.is_loopback() || ip.is_unicast_link_local() || ip.is_unspecified()
}

fn domain_host_is_risky(domain: &str) -> bool {
    if let Ok(ip) = domain.parse::<IpAddr>() {
        return ip_is_blocked_private_adjacent(ip);
    }
    let lower = domain.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return true;
    }
    if lower == "metadata.google.internal" {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_subscription_serialization() {
        // Test snake_case serialization
        let sub = WebhookEventSubscription::TaskCreated;
        assert_eq!(serde_json::to_string(&sub).unwrap(), "\"task_created\"");

        // Test wildcard serialization
        let wild = WebhookEventSubscription::Wildcard;
        assert_eq!(serde_json::to_string(&wild).unwrap(), "\"*\"");
    }

    #[test]
    fn test_event_subscription_deserialization() {
        let sub: WebhookEventSubscription = serde_json::from_str("\"task_created\"").unwrap();
        assert_eq!(sub, WebhookEventSubscription::TaskCreated);

        let wild: WebhookEventSubscription = serde_json::from_str("\"*\"").unwrap();
        assert_eq!(wild, WebhookEventSubscription::Wildcard);
    }

    #[test]
    fn test_invalid_event_rejected() {
        let result: Result<WebhookEventSubscription, _> = serde_json::from_str("\"task_creatd\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_event_enabled_with_subscription_type() {
        let config = WebhookConfig {
            enabled: Some(true),
            events: Some(vec![
                WebhookEventSubscription::TaskCreated,
                WebhookEventSubscription::Wildcard,
            ]),
            ..Default::default()
        };
        assert!(config.is_event_enabled("task_created"));
        assert!(config.is_event_enabled("loop_started")); // via wildcard
    }

    #[test]
    fn test_is_event_enabled_default_events_when_none() {
        let config = WebhookConfig {
            enabled: Some(true),
            events: None,
            ..Default::default()
        };
        assert!(config.is_event_enabled("task_created"));
        assert!(config.is_event_enabled("task_started"));
        assert!(!config.is_event_enabled("loop_started")); // not in default set
    }

    #[test]
    fn test_is_event_enabled_disabled_when_not_enabled() {
        let config = WebhookConfig {
            enabled: Some(false),
            events: Some(vec![WebhookEventSubscription::TaskCreated]),
            ..Default::default()
        };
        assert!(!config.is_event_enabled("task_created"));
    }

    #[test]
    fn validate_destination_accepts_public_https() {
        validate_webhook_destination_url("https://hooks.example.com/ralph", false, false).unwrap();
    }

    #[test]
    fn validate_destination_rejects_http_by_default() {
        let err = validate_webhook_destination_url("http://hooks.example.com/ralph", false, false)
            .unwrap_err();
        assert!(err.to_string().contains("http://"));
    }

    #[test]
    fn validate_destination_allows_http_when_opted_in() {
        validate_webhook_destination_url("http://hooks.example.com/ralph", true, false).unwrap();
    }

    #[test]
    fn validate_destination_rejects_loopback_https() {
        assert!(validate_webhook_destination_url("https://127.0.0.1/hook", false, false).is_err());
        assert!(validate_webhook_destination_url("https://[::1]/hook", false, false).is_err());
    }

    #[test]
    fn validate_destination_rejects_link_local_ipv4() {
        assert!(
            validate_webhook_destination_url("https://169.254.169.254/latest", false, false)
                .is_err()
        );
    }

    #[test]
    fn validate_destination_rejects_metadata_hostname() {
        assert!(
            validate_webhook_destination_url("https://metadata.google.internal/", false, false)
                .is_err()
        );
    }

    #[test]
    fn validate_destination_allows_risky_targets_when_opted_in() {
        validate_webhook_destination_url("https://127.0.0.1/hook", false, true).unwrap();
        validate_webhook_destination_url("http://127.0.0.1/hook", true, true).unwrap();
    }

    #[test]
    fn validate_settings_skips_url_when_disabled() {
        let cfg = WebhookConfig {
            enabled: Some(false),
            url: Some("https://127.0.0.1/nope".to_string()),
            ..Default::default()
        };
        validate_webhook_settings(&cfg).unwrap();
    }

    #[test]
    fn validate_settings_requires_url_when_enabled() {
        let cfg = WebhookConfig {
            enabled: Some(true),
            url: None,
            ..Default::default()
        };
        assert!(validate_webhook_settings(&cfg).is_err());
    }
}
