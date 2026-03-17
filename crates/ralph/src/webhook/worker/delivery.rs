//! Webhook delivery helpers.
//!
//! Responsibilities:
//! - Execute webhook delivery attempts, including request serialization and signature generation.
//! - Schedule retries by returning work to the runtime-owned retry queue.
//! - Render destinations safely for logs, diagnostics, and persisted failure records.
//!
//! Not handled here:
//! - Dispatcher lifecycle or worker-pool sizing.
//! - Backpressure policy selection for new messages.
//! - Replay filtering or failure-store retention.
//!
//! Invariants/assumptions:
//! - Every user-visible destination string is redacted before logging or persistence.
//! - Retry delays are deterministic from configured backoff and attempt number.
//! - Test transport injection stays crate-local and fully in-process.

use anyhow::Context;
use crossbeam_channel::Sender;
use std::time::{Duration, Instant};

#[cfg(test)]
use std::sync::{Arc, OnceLock, RwLock};

use super::super::diagnostics;
use super::super::types::WebhookMessage;
use super::runtime::{DeliveryTask, ScheduledRetry};

pub(super) fn handle_delivery_task(task: DeliveryTask, retry_sender: &Sender<ScheduledRetry>) {
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
    let millis = base
        .as_millis()
        .saturating_mul(retry_number as u128)
        .min(u64::MAX as u128) as u64;
    Duration::from_millis(millis)
}

fn deliver_attempt(msg: &WebhookMessage) -> anyhow::Result<()> {
    let url = msg
        .config
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Webhook URL not configured"))?;
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
    use hmac::{Hmac, Mac};
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
