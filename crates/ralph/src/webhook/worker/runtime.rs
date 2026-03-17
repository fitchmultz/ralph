//! Webhook dispatcher runtime orchestration.
//!
//! Responsibilities:
//! - Own the reloadable dispatcher state, worker-pool lifecycle, and retry scheduler thread.
//! - Scale queue capacity deterministically from active mode + webhook config.
//! - Route ready delivery tasks to delivery helpers without blocking enqueue callers.
//!
//! Not handled here:
//! - HTTP request construction or signature generation.
//! - Queue backpressure policy decisions for new messages.
//! - Failure-store persistence or replay filtering.
//!
//! Invariants/assumptions:
//! - Runtime settings are rebuilt when the effective mode/config changes.
//! - Retry scheduling stays off worker threads so failing endpoints do not sleep in place.
//! - Dispatcher shutdown is best-effort and relies on channel teardown via `Arc` drop.

use crate::contracts::WebhookConfig;
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use super::super::diagnostics;
use super::super::types::WebhookMessage;
use super::delivery::handle_delivery_task;

const DEFAULT_QUEUE_CAPACITY: usize = 500;
const DEFAULT_WORKER_COUNT: usize = 4;
const MAX_QUEUE_CAPACITY: usize = 10_000;
const MAX_PARALLEL_MULTIPLIER: f64 = 10.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DispatcherSettings {
    pub(super) queue_capacity: usize,
    pub(super) worker_count: usize,
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
pub(super) struct WebhookDispatcher {
    pub(super) settings: DispatcherSettings,
    pub(super) ready_sender: Sender<DeliveryTask>,
    retry_sender: Sender<ScheduledRetry>,
}

#[derive(Debug, Clone)]
pub(super) struct DeliveryTask {
    pub(super) msg: WebhookMessage,
    pub(super) attempt: u32,
}

#[derive(Debug, Clone)]
pub(super) struct ScheduledRetry {
    pub(super) ready_at: Instant,
    pub(super) task: DeliveryTask,
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

pub(super) fn dispatcher_for_config(config: &WebhookConfig) -> Arc<WebhookDispatcher> {
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
    super::delivery::install_test_transport_for_tests(None);
}
