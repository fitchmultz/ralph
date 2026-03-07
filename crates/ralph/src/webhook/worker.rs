//! Webhook worker runtime and delivery logic.
//!
//! Responsibilities:
//! - Manage the reloadable in-process webhook dispatcher and worker pool.
//! - Handle HTTP delivery attempts, retry scheduling, and HMAC signatures.
//! - Apply backpressure policies for the ready queue before work enters the pool.
//!
//! Not handled here:
//! - Type definitions (see `super::types`).
//! - Notification convenience functions (see `super::notifications`).
//! - Diagnostics persistence and replay selection (see `super::diagnostics`).
//!
//! Invariants/assumptions:
//! - Delivery runtime settings are derived deterministically from the active mode + config.
//! - Retries are scheduled off the hot worker path so one failing endpoint does not stall peers.
//! - All human-visible destinations must be rendered through `redact_webhook_destination`.

use crate::contracts::{WebhookConfig, WebhookQueuePolicy};
use anyhow::Context;
use crossbeam_channel::{Receiver, SendTimeoutError, Sender, TrySendError, bounded, unbounded};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use super::diagnostics;
use super::types::{ResolvedWebhookConfig, WebhookMessage, WebhookPayload};

const DEFAULT_QUEUE_CAPACITY: usize = 500;
const DEFAULT_WORKER_COUNT: usize = 4;
const MAX_QUEUE_CAPACITY: usize = 10_000;
const MAX_PARALLEL_MULTIPLIER: f64 = 10.0;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DispatcherSettings {
    queue_capacity: usize,
    worker_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeMode {
    Standard,
    Parallel { worker_count: u8 },
}

#[derive(Debug)]
struct DispatcherState {
    mode: RuntimeMode,
    dispatcher: Option<Arc<WebhookDispatcher>>,
}

impl Default for DispatcherState {
    fn default() -> Self {
        Self {
            mode: RuntimeMode::Standard,
            dispatcher: None,
        }
    }
}

#[derive(Debug)]
struct WebhookDispatcher {
    settings: DispatcherSettings,
    ready_sender: Sender<DeliveryTask>,
    retry_sender: Sender<ScheduledRetry>,
}

#[derive(Debug, Clone)]
struct DeliveryTask {
    msg: WebhookMessage,
    attempt: u32,
}

#[derive(Debug, Clone)]
struct ScheduledRetry {
    ready_at: Instant,
    task: DeliveryTask,
}

#[derive(Debug, Clone)]
struct RetryQueueEntry(ScheduledRetry);

impl PartialEq for RetryQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.0.ready_at.eq(&other.0.ready_at)
    }
}

impl Eq for RetryQueueEntry {}

impl PartialOrd for RetryQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for RetryQueueEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        other.0.ready_at.cmp(&self.0.ready_at)
    }
}

static DISPATCHER_STATE: OnceLock<RwLock<DispatcherState>> = OnceLock::new();

fn dispatcher_state() -> &'static RwLock<DispatcherState> {
    DISPATCHER_STATE.get_or_init(|| RwLock::new(DispatcherState::default()))
}

impl DispatcherSettings {
    fn for_mode(config: &WebhookConfig, mode: &RuntimeMode) -> Self {
        let base_capacity = config
            .queue_capacity
            .map(|value| value.clamp(1, MAX_QUEUE_CAPACITY as u32) as usize)
            .unwrap_or(DEFAULT_QUEUE_CAPACITY);

        match mode {
            RuntimeMode::Standard => Self {
                queue_capacity: base_capacity,
                worker_count: DEFAULT_WORKER_COUNT,
            },
            RuntimeMode::Parallel { worker_count } => {
                let multiplier = config
                    .parallel_queue_multiplier
                    .unwrap_or(2.0)
                    .clamp(1.0, MAX_PARALLEL_MULTIPLIER as f32)
                    as f64;
                let scaled_capacity =
                    (base_capacity as f64 * (*worker_count as f64 * multiplier).max(1.0)) as usize;

                Self {
                    queue_capacity: scaled_capacity.clamp(1, MAX_QUEUE_CAPACITY),
                    worker_count: usize::max(DEFAULT_WORKER_COUNT, *worker_count as usize),
                }
            }
        }
    }
}

impl WebhookDispatcher {
    fn new(settings: DispatcherSettings) -> Arc<Self> {
        let (ready_sender, ready_receiver) = bounded(settings.queue_capacity);
        let (retry_sender, retry_receiver) = unbounded();

        let dispatcher = Arc::new(Self {
            settings: settings.clone(),
            ready_sender,
            retry_sender,
        });

        diagnostics::set_queue_capacity(settings.queue_capacity);

        for worker_id in 0..settings.worker_count {
            let ready_receiver = ready_receiver.clone();
            let retry_sender = dispatcher.retry_sender.clone();
            let thread_name = format!("ralph-webhook-worker-{worker_id}");
            std::thread::Builder::new()
                .name(thread_name)
                .spawn(move || worker_loop(ready_receiver, retry_sender))
                .expect("spawn webhook delivery worker");
        }

        let scheduler_ready = dispatcher.ready_sender.clone();
        std::thread::Builder::new()
            .name("ralph-webhook-retry-scheduler".to_string())
            .spawn(move || retry_scheduler_loop(retry_receiver, scheduler_ready))
            .expect("spawn webhook retry scheduler");

        log::debug!(
            "Webhook dispatcher started with {} workers and queue capacity {}",
            settings.worker_count,
            settings.queue_capacity
        );

        dispatcher
    }
}

impl Drop for WebhookDispatcher {
    fn drop(&mut self) {
        log::debug!(
            "Webhook dispatcher shutting down (workers: {}, capacity: {})",
            self.settings.worker_count,
            self.settings.queue_capacity
        );
    }
}

fn with_dispatcher_state_write<T>(mut f: impl FnMut(&mut DispatcherState) -> T) -> T {
    match dispatcher_state().write() {
        Ok(mut guard) => f(&mut guard),
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            f(&mut guard)
        }
    }
}

fn dispatcher_for_config(config: &WebhookConfig) -> Arc<WebhookDispatcher> {
    with_dispatcher_state_write(|state| {
        let settings = DispatcherSettings::for_mode(config, &state.mode);
        let needs_rebuild = state
            .dispatcher
            .as_ref()
            .is_none_or(|dispatcher| dispatcher.settings != settings);

        if needs_rebuild {
            state.dispatcher = Some(WebhookDispatcher::new(settings));
        }

        state
            .dispatcher
            .as_ref()
            .expect("dispatcher initialized")
            .clone()
    })
}

/// Initialize the webhook dispatcher with capacity scaled for parallel execution.
pub fn init_worker_for_parallel(config: &WebhookConfig, worker_count: u8) {
    with_dispatcher_state_write(|state| {
        state.mode = RuntimeMode::Parallel { worker_count };
    });
    let _ = dispatcher_for_config(config);
}

fn worker_loop(ready_receiver: Receiver<DeliveryTask>, retry_sender: Sender<ScheduledRetry>) {
    while let Ok(task) = ready_receiver.recv() {
        diagnostics::note_queue_dequeue();
        handle_delivery_task(task, &retry_sender);
    }
}

fn handle_delivery_task(task: DeliveryTask, retry_sender: &Sender<ScheduledRetry>) {
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

fn retry_scheduler_loop(
    retry_receiver: Receiver<ScheduledRetry>,
    ready_sender: Sender<DeliveryTask>,
) {
    let mut pending = BinaryHeap::<RetryQueueEntry>::new();

    loop {
        let timeout = pending
            .peek()
            .map(|entry| entry.0.ready_at.saturating_duration_since(Instant::now()));

        let scheduled = match timeout {
            Some(duration) => match retry_receiver.recv_timeout(duration) {
                Ok(task) => Some(task),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    if pending.is_empty() {
                        break;
                    }
                    None
                }
            },
            None => match retry_receiver.recv() {
                Ok(task) => Some(task),
                Err(_) => break,
            },
        };

        if let Some(task) = scheduled {
            pending.push(RetryQueueEntry(task));
        }

        let now = Instant::now();
        while let Some(entry) = pending.peek() {
            if entry.0.ready_at > now {
                break;
            }

            let RetryQueueEntry(scheduled) = pending.pop().expect("pending retry exists");
            match ready_sender.send(scheduled.task.clone()) {
                Ok(()) => diagnostics::note_retry_requeue(),
                Err(send_err) => {
                    let error = anyhow::anyhow!(
                        "webhook dispatcher shut down before retry enqueue: {send_err}"
                    );
                    diagnostics::note_delivery_failure(
                        &scheduled.task.msg,
                        &error,
                        scheduled.task.attempt.saturating_add(1),
                    );
                    log::warn!("{error:#}");
                    return;
                }
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

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct TestRequest {
    pub(crate) url: String,
    pub(crate) body: String,
    pub(crate) signature: Option<String>,
    pub(crate) timeout: Duration,
}

#[cfg(test)]
type TestTransportHandler = Arc<dyn Fn(&TestRequest) -> anyhow::Result<()> + Send + Sync + 'static>;

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
pub(crate) fn current_dispatcher_settings_for_tests(config: &WebhookConfig) -> (usize, usize) {
    let dispatcher = dispatcher_for_config(config);
    (
        dispatcher.settings.queue_capacity,
        dispatcher.settings.worker_count,
    )
}

#[cfg(test)]
pub(crate) fn reset_dispatcher_for_tests() {
    with_dispatcher_state_write(|state| {
        state.mode = RuntimeMode::Standard;
        state.dispatcher = None;
    });
    install_test_transport_for_tests(None);
}
