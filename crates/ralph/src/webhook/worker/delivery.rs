//! Purpose: Execute webhook delivery attempts and hand retry work back to the runtime scheduler.
//!
//! Responsibilities:
//! - Serialize webhook requests, sign payloads, and perform delivery attempts.
//! - Schedule retries through the runtime-owned retry queue without sleeping on worker threads.
//! - Render destinations safely for logs, diagnostics, and persisted failure records.
//!
//! Scope:
//! - Delivery-attempt execution, retry handoff, redaction, and test transport injection only.
//!
//! Usage:
//! - Called by webhook runtime workers for normal delivery and retry processing.
//!
//! Invariants/Assumptions:
//! - Every user-visible destination string is redacted before logging or persistence.
//! - Retry delays use exponential backoff with bounded jitter and a fixed cap.
//! - Test transport injection stays crate-local and fully in-process.

use anyhow::Context;

use crate::contracts::validate_webhook_destination_url;
use crossbeam_channel::Sender;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Weak;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use std::sync::{Arc, OnceLock, RwLock};

use super::super::diagnostics;
use super::super::types::WebhookMessage;
use super::runtime::{DeliveryTask, ScheduledRetry};

const WEBHOOK_RETRY_MAX_DELAY: Duration = Duration::from_secs(30);
const WEBHOOK_RETRY_JITTER_PER_MILLE: i32 = 200;

static WEBHOOK_RETRY_JITTER_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn handle_delivery_task(
    task: DeliveryTask,
    retry_sender: &Weak<Sender<ScheduledRetry>>,
) {
    match deliver_attempt(&task.msg) {
        Ok(()) => {
            diagnostics::note_delivery_success();
            log::debug!(
                "Webhook delivered successfully to {}",
                redact_webhook_destination(
                    task.msg
                        .config
                        .url
                        .as_deref()
                        .unwrap_or("<missing webhook URL>")
                )
            );
        }
        Err(err) => {
            if task.attempt < task.msg.config.retry_count {
                diagnostics::note_retry_attempt();

                let retry_number = task.attempt.saturating_add(1);
                let scheduled = ScheduledRetry {
                    ready_at: Instant::now()
                        + retry_delay(task.msg.config.retry_backoff, retry_number),
                    task: DeliveryTask {
                        msg: task.msg.clone(),
                        attempt: retry_number,
                    },
                };

                log::debug!(
                    "Webhook attempt {} failed for {}; scheduling retry: {:#}",
                    retry_number,
                    redact_webhook_destination(
                        task.msg
                            .config
                            .url
                            .as_deref()
                            .unwrap_or("<missing webhook URL>")
                    ),
                    err
                );

                let Some(retry_sender) = retry_sender.upgrade() else {
                    let scheduler_error = anyhow::anyhow!(
                        "retry scheduler unavailable for webhook: dispatcher shutting down"
                    );
                    diagnostics::note_delivery_failure(
                        &task.msg,
                        &scheduler_error,
                        retry_number.saturating_add(1),
                    );
                    log::warn!("{scheduler_error:#}");
                    return;
                };

                if let Err(send_err) = retry_sender.send(scheduled) {
                    let scheduler_error =
                        anyhow::anyhow!("retry scheduler unavailable for webhook: {}", send_err);
                    diagnostics::note_delivery_failure(
                        &task.msg,
                        &scheduler_error,
                        retry_number.saturating_add(1),
                    );
                    log::warn!("{scheduler_error:#}");
                }
            } else {
                let attempts = task.attempt.saturating_add(1);
                diagnostics::note_delivery_failure(&task.msg, &err, attempts);
                log::warn!(
                    "Webhook delivery failed after {} attempts: {:#}",
                    attempts,
                    err
                );
            }
        }
    }
}

fn retry_delay(base: Duration, retry_number: u32) -> Duration {
    retry_delay_with_jitter(base, retry_number, random_jitter_per_mille())
}

fn retry_delay_with_jitter(base: Duration, retry_number: u32, jitter_per_mille: i32) -> Duration {
    let multiplier = exponential_retry_multiplier(retry_number);
    let millis = base
        .as_millis()
        .saturating_mul(multiplier)
        .min(WEBHOOK_RETRY_MAX_DELAY.as_millis());
    let bounded_jitter = jitter_per_mille.clamp(
        -WEBHOOK_RETRY_JITTER_PER_MILLE,
        WEBHOOK_RETRY_JITTER_PER_MILLE,
    );
    let jittered = apply_jitter(millis, bounded_jitter)
        .min(WEBHOOK_RETRY_MAX_DELAY.as_millis())
        .min(u64::MAX as u128) as u64;

    Duration::from_millis(jittered)
}

fn exponential_retry_multiplier(retry_number: u32) -> u128 {
    if retry_number == 0 {
        return 0;
    }

    1u128
        .checked_shl(retry_number.saturating_sub(1))
        .unwrap_or(u128::MAX)
}

fn apply_jitter(millis: u128, jitter_per_mille: i32) -> u128 {
    let scale = 1000i128 + jitter_per_mille as i128;
    if scale <= 0 {
        return 0;
    }

    millis.saturating_mul(scale as u128) / 1000
}

fn random_jitter_per_mille() -> i32 {
    let mut hasher = DefaultHasher::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = WEBHOOK_RETRY_JITTER_COUNTER.fetch_add(1, Ordering::Relaxed);

    now.hash(&mut hasher);
    counter.hash(&mut hasher);
    std::process::id().hash(&mut hasher);

    let span = (WEBHOOK_RETRY_JITTER_PER_MILLE as u64).saturating_mul(2);
    (hasher.finish() % (span + 1)) as i32 - WEBHOOK_RETRY_JITTER_PER_MILLE
}

fn deliver_attempt(msg: &WebhookMessage) -> anyhow::Result<()> {
    let url = msg
        .config
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Webhook URL not configured"))?;
    validate_webhook_destination_url(
        url,
        msg.config.allow_insecure_http,
        msg.config.allow_private_targets,
    )
    .context("webhook URL failed safety validation")?;
    let destination = redact_webhook_destination(url);

    let body = serde_json::to_string(&msg.payload)?;
    let signature = msg
        .config
        .secret
        .as_ref()
        .map(|secret| generate_signature(&body, secret));

    send_request(url, &body, signature.as_deref(), msg.config.timeout)
        .with_context(|| format!("webhook delivery to {destination}"))
}

fn send_request(
    url: &str,
    body: &str,
    signature: Option<&str>,
    timeout: Duration,
) -> anyhow::Result<()> {
    #[cfg(test)]
    if let Some(handler) = test_transport() {
        return handler(&TestRequest {
            url: url.to_string(),
            body: body.to_string(),
            signature: signature.map(std::string::ToString::to_string),
            timeout,
        });
    }

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

/// Render a webhook destination for logs and diagnostics without leaking secrets.
pub(crate) fn redact_webhook_destination(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return "<missing webhook URL>".to_string();
    }

    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);

    if let Some((scheme, rest)) = without_query.split_once("://") {
        let authority_and_path = rest.trim_start_matches('/');
        let authority = authority_and_path
            .split('/')
            .next()
            .unwrap_or(authority_and_path)
            .split('@')
            .next_back()
            .unwrap_or(authority_and_path);

        if authority.is_empty() {
            return format!("{scheme}://<redacted>");
        }

        let has_path = authority_and_path.len() > authority.len();
        return if has_path {
            format!("{scheme}://{authority}/…")
        } else {
            format!("{scheme}://{authority}")
        };
    }

    let without_userinfo = without_query
        .split('@')
        .next_back()
        .unwrap_or(without_query);
    let host = without_userinfo
        .split('/')
        .next()
        .unwrap_or(without_userinfo);

    if host.is_empty() {
        "<redacted webhook destination>".to_string()
    } else if without_userinfo.len() > host.len() {
        format!("{host}/…")
    } else {
        host.to_string()
    }
}

/// Generate HMAC-SHA256 signature for webhook payload.
pub(crate) fn generate_signature(body: &str, secret: &str) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(mac) => mac,
        Err(e) => {
            log::error!("Failed to create HMAC (this should never happen): {}", e);
            return "sha256=invalid".to_string();
        }
    };
    mac.update(body.as_bytes());
    let result = mac.finalize();
    let code_bytes = result.into_bytes();

    format!("sha256={}", hex::encode(code_bytes))
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct TestRequest {
    pub(crate) url: String,
    pub(crate) body: String,
    pub(crate) signature: Option<String>,
    pub(crate) timeout: Duration,
}

#[cfg(test)]
pub(crate) type TestTransportHandler =
    Arc<dyn Fn(&TestRequest) -> anyhow::Result<()> + Send + Sync + 'static>;

#[cfg(test)]
static TEST_TRANSPORT: OnceLock<RwLock<Option<TestTransportHandler>>> = OnceLock::new();

#[cfg(test)]
fn test_transport() -> Option<TestTransportHandler> {
    let lock = TEST_TRANSPORT.get_or_init(|| RwLock::new(None));
    match lock.read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

#[cfg(test)]
pub(crate) fn install_test_transport_for_tests(handler: Option<TestTransportHandler>) {
    let lock = TEST_TRANSPORT.get_or_init(|| RwLock::new(None));
    match lock.write() {
        Ok(mut guard) => *guard = handler,
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            *guard = handler;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_uses_exponential_sequence_without_jitter() {
        let base = Duration::from_millis(100);

        assert_eq!(
            retry_delay_with_jitter(base, 1, 0),
            Duration::from_millis(100)
        );
        assert_eq!(
            retry_delay_with_jitter(base, 2, 0),
            Duration::from_millis(200)
        );
        assert_eq!(
            retry_delay_with_jitter(base, 3, 0),
            Duration::from_millis(400)
        );
        assert_eq!(
            retry_delay_with_jitter(base, 4, 0),
            Duration::from_millis(800)
        );
    }

    #[test]
    fn retry_delay_applies_bounded_jitter() {
        let base = Duration::from_millis(1000);

        assert_eq!(
            retry_delay_with_jitter(base, 1, -WEBHOOK_RETRY_JITTER_PER_MILLE),
            Duration::from_millis(800)
        );
        assert_eq!(
            retry_delay_with_jitter(base, 1, WEBHOOK_RETRY_JITTER_PER_MILLE),
            Duration::from_millis(1200)
        );
        assert_eq!(
            retry_delay_with_jitter(base, 1, -999),
            Duration::from_millis(800)
        );
        assert_eq!(
            retry_delay_with_jitter(base, 1, 999),
            Duration::from_millis(1200)
        );
    }

    #[test]
    fn retry_delay_caps_final_delay_after_jitter() {
        let base = Duration::from_millis(10_000);

        assert_eq!(
            retry_delay_with_jitter(base, 4, WEBHOOK_RETRY_JITTER_PER_MILLE),
            WEBHOOK_RETRY_MAX_DELAY
        );
        assert_eq!(
            retry_delay_with_jitter(base, 10, 0),
            WEBHOOK_RETRY_MAX_DELAY
        );
    }

    #[test]
    fn retry_delay_handles_extreme_retry_numbers() {
        let delay = retry_delay_with_jitter(
            Duration::from_millis(u64::MAX),
            u32::MAX,
            WEBHOOK_RETRY_JITTER_PER_MILLE,
        );

        assert_eq!(delay, WEBHOOK_RETRY_MAX_DELAY);
    }
}
