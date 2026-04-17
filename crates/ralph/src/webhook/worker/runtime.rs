//! Purpose: Own the reloadable webhook dispatcher runtime and its worker/scheduler lifecycle.
//!
//! Responsibilities:
//! - Build and rebuild dispatcher state from webhook runtime mode and config.
//! - Start delivery workers and the retry scheduler with deterministic startup behavior.
//! - Route ready delivery tasks to delivery helpers without blocking enqueue callers.
//!
//! Scope:
//! - Dispatcher lifecycle, thread startup/teardown, queue sizing, and retry scheduling orchestration.
//!
//! Usage:
//! - Called by webhook enqueue helpers and test-only runtime controls through the worker facade.
//!
//! Invariants/Assumptions:
//! - Runtime settings are rebuilt when the effective mode/config changes.
//! - Retry scheduling stays off worker threads so failing endpoints do not sleep in place.
//! - Dispatcher teardown must not leak background threads or retain stale queue channels across rebuilds.
//! - When the inbound retry channel disconnects during a rebuild, the scheduler still honors pending
//!   `ready_at` deadlines before exiting so in-flight retries are not dropped.

use crate::contracts::WebhookConfig;
use anyhow::Context;
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, unbounded};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::io;
use std::sync::{Arc, OnceLock, RwLock, Weak};
use std::time::{Duration, Instant};

use super::super::diagnostics;
use super::super::types::WebhookMessage;
use super::delivery::handle_delivery_task;

const DEFAULT_QUEUE_CAPACITY: usize = 500;
const DEFAULT_WORKER_COUNT: usize = 4;
const MAX_QUEUE_CAPACITY: usize = 10_000;
const MAX_PARALLEL_MULTIPLIER: f64 = 10.0;
const DISPATCHER_STARTUP_TIMEOUT: Duration = Duration::from_secs(1);

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
    disabled_reason: Option<String>,
}

impl Default for DispatcherState {
    fn default() -> Self {
        Self {
            mode: RuntimeMode::Standard,
            dispatcher: None,
            disabled_reason: None,
        }
    }
}

#[derive(Debug)]
pub(super) struct WebhookDispatcher {
    pub(super) settings: DispatcherSettings,
    pub(super) ready_sender: Arc<Sender<DeliveryTask>>,
    retry_sender: Arc<Sender<ScheduledRetry>>,
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

trait ThreadSpawner {
    fn spawn(&self, name: String, task: Box<dyn FnOnce() + Send + 'static>) -> io::Result<()>;
}

#[derive(Debug, Default)]
struct OsThreadSpawner;

impl ThreadSpawner for OsThreadSpawner {
    fn spawn(&self, name: String, task: Box<dyn FnOnce() + Send + 'static>) -> io::Result<()> {
        std::thread::Builder::new()
            .name(name)
            .spawn(task)
            .map(|_| ())
    }
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
    fn new(settings: DispatcherSettings) -> anyhow::Result<Arc<Self>> {
        Self::new_with_spawner(settings, &OsThreadSpawner)
    }

    fn new_with_spawner(
        settings: DispatcherSettings,
        spawner: &impl ThreadSpawner,
    ) -> anyhow::Result<Arc<Self>> {
        let (ready_sender, ready_receiver) = bounded(settings.queue_capacity);
        let (retry_sender, retry_receiver) = unbounded();
        let startup_signals = settings.worker_count.saturating_add(1);
        let (startup_sender, startup_receiver) = bounded(startup_signals);

        let dispatcher = Arc::new(Self {
            settings: settings.clone(),
            ready_sender: Arc::new(ready_sender),
            retry_sender: Arc::new(retry_sender),
        });

        for worker_id in 0..settings.worker_count {
            let ready_receiver = ready_receiver.clone();
            let retry_sender = Arc::downgrade(&dispatcher.retry_sender);
            let startup_sender = startup_sender.clone();
            let thread_name = format!("ralph-webhook-worker-{worker_id}");
            spawner
                .spawn(
                    thread_name,
                    Box::new(move || {
                        if let Err(err) = startup_sender.send(()) {
                            log::warn!(
                                "Webhook delivery worker startup signal skipped because dispatcher startup was abandoned: {err}"
                            );
                            return;
                        }

                        worker_loop(ready_receiver, retry_sender)
                    }),
                )
                .with_context(|| format!("spawn webhook delivery worker {worker_id}"))?;
        }

        let scheduler_ready = Arc::downgrade(&dispatcher.ready_sender);
        let scheduler_startup_sender = startup_sender.clone();
        spawner
            .spawn(
                "ralph-webhook-retry-scheduler".to_string(),
                Box::new(move || {
                    if let Err(err) = scheduler_startup_sender.send(()) {
                        log::warn!(
                            "Webhook retry scheduler startup signal skipped because dispatcher startup was abandoned: {err}"
                        );
                        return;
                    }

                    retry_scheduler_loop(retry_receiver, scheduler_ready)
                }),
            )
            .context("spawn webhook retry scheduler")?;
        drop(startup_sender);

        for started_count in 0..startup_signals {
            if let Err(err) = startup_receiver.recv_timeout(DISPATCHER_STARTUP_TIMEOUT) {
                anyhow::bail!(
                    "wait for webhook dispatcher thread startup ({started_count}/{startup_signals} ready): {err}"
                );
            }
        }

        diagnostics::set_queue_capacity(settings.queue_capacity);

        log::debug!(
            "Webhook dispatcher started with {} workers and queue capacity {}",
            settings.worker_count,
            settings.queue_capacity
        );

        Ok(dispatcher)
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

pub(super) fn dispatcher_for_config(config: &WebhookConfig) -> Option<Arc<WebhookDispatcher>> {
    dispatcher_for_config_with_factory(config, WebhookDispatcher::new)
}

#[cfg(test)]
fn dispatcher_for_config_with_spawner(
    config: &WebhookConfig,
    spawner: &impl ThreadSpawner,
) -> Option<Arc<WebhookDispatcher>> {
    dispatcher_for_config_with_factory(config, |settings| {
        WebhookDispatcher::new_with_spawner(settings, spawner)
    })
}

fn dispatcher_for_config_with_factory(
    config: &WebhookConfig,
    mut build_dispatcher: impl FnMut(DispatcherSettings) -> anyhow::Result<Arc<WebhookDispatcher>>,
) -> Option<Arc<WebhookDispatcher>> {
    with_dispatcher_state_write(|state| {
        if state.disabled_reason.is_some() {
            log::debug!("Webhooks disabled for this run after dispatcher startup failure");
            return None;
        }

        let settings = DispatcherSettings::for_mode(config, &state.mode);
        let needs_rebuild = state
            .dispatcher
            .as_ref()
            .is_none_or(|dispatcher| dispatcher.settings != settings);

        if needs_rebuild {
            match build_dispatcher(settings) {
                Ok(dispatcher) => state.dispatcher = Some(dispatcher),
                Err(err) => {
                    let reason = format!("{err:#}");
                    state.dispatcher = None;
                    state.disabled_reason = Some(reason.clone());
                    diagnostics::set_queue_capacity(0);
                    log::warn!(
                        "Webhook delivery disabled for this run: failed to start dispatcher runtime: {reason}"
                    );
                    return None;
                }
            }
        }

        state.dispatcher.as_ref().cloned()
    })
}

/// Initialize the webhook dispatcher with capacity scaled for parallel execution.
pub fn init_worker_for_parallel(config: &WebhookConfig, worker_count: u8) {
    with_dispatcher_state_write(|state| {
        state.mode = RuntimeMode::Parallel { worker_count };
    });
    let _ = dispatcher_for_config(config);
}

fn worker_loop(ready_receiver: Receiver<DeliveryTask>, retry_sender: Weak<Sender<ScheduledRetry>>) {
    while let Ok(task) = ready_receiver.recv() {
        diagnostics::note_queue_dequeue();
        handle_delivery_task(task, &retry_sender);
    }
}

fn retry_scheduler_loop(
    retry_receiver: Receiver<ScheduledRetry>,
    ready_sender: Weak<Sender<DeliveryTask>>,
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
                    // Inbound retry channel closed (dispatcher rebuild/teardown) while we still have
                    // timer-delayed retries: `recv_timeout` returns `Disconnected` immediately on every
                    // call, so we must wait out `ready_at` deadlines here or those retries never enqueue.
                    let wait = pending
                        .peek()
                        .map(|entry| entry.0.ready_at.saturating_duration_since(Instant::now()));
                    if let Some(delay) = wait
                        && !delay.is_zero()
                    {
                        std::thread::sleep(delay);
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

            let Some(RetryQueueEntry(scheduled)) = pending.pop() else {
                break;
            };
            let Some(ready_sender) = ready_sender.upgrade() else {
                let error = anyhow::anyhow!(
                    "webhook dispatcher shut down before retry enqueue: ready queue unavailable"
                );
                diagnostics::note_delivery_failure(
                    &scheduled.task.msg,
                    &error,
                    scheduled.task.attempt.saturating_add(1),
                );
                log::warn!("{error:#}");
                return;
            };

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

        if pending.is_empty() {
            match retry_receiver.try_recv() {
                Err(TryRecvError::Disconnected) => break,
                Ok(_) | Err(TryRecvError::Empty) => {}
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn current_dispatcher_settings_for_tests(
    config: &WebhookConfig,
) -> Option<(usize, usize)> {
    dispatcher_for_config(config).map(|dispatcher| {
        (
            dispatcher.settings.queue_capacity,
            dispatcher.settings.worker_count,
        )
    })
}

#[cfg(test)]
pub(crate) fn reset_dispatcher_for_tests() {
    with_dispatcher_state_write(|state| {
        state.mode = RuntimeMode::Standard;
        state.dispatcher = None;
        state.disabled_reason = None;
    });
    super::delivery::install_test_transport_for_tests(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct FailingThreadSpawner;

    impl ThreadSpawner for FailingThreadSpawner {
        fn spawn(
            &self,
            _name: String,
            _task: Box<dyn FnOnce() + Send + 'static>,
        ) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "simulated thread exhaustion",
            ))
        }
    }

    #[derive(Debug, Default)]
    struct SilentThreadSpawner;

    impl ThreadSpawner for SilentThreadSpawner {
        fn spawn(
            &self,
            _name: String,
            _task: Box<dyn FnOnce() + Send + 'static>,
        ) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct CountingSpawner {
        calls: AtomicUsize,
    }

    impl ThreadSpawner for CountingSpawner {
        fn spawn(
            &self,
            _name: String,
            _task: Box<dyn FnOnce() + Send + 'static>,
        ) -> io::Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(io::Error::other(
                "dispatcher should stay disabled after startup failure",
            ))
        }
    }

    #[test]
    #[serial]
    fn thread_spawn_failure_disables_webhooks_for_run_without_panic() {
        reset_dispatcher_for_tests();
        let config = WebhookConfig::default();

        let dispatcher = dispatcher_for_config_with_spawner(&config, &FailingThreadSpawner);
        assert!(dispatcher.is_none());

        let counting_spawner = CountingSpawner::default();
        let retry = dispatcher_for_config_with_spawner(&config, &counting_spawner);
        assert!(retry.is_none());
        assert_eq!(counting_spawner.calls.load(Ordering::SeqCst), 0);

        reset_dispatcher_for_tests();
    }

    #[test]
    #[serial]
    fn startup_handshake_timeout_disables_webhooks_without_panic() {
        reset_dispatcher_for_tests();
        let config = WebhookConfig::default();

        let dispatcher = dispatcher_for_config_with_spawner(&config, &SilentThreadSpawner);
        assert!(dispatcher.is_none());

        reset_dispatcher_for_tests();
    }
}
