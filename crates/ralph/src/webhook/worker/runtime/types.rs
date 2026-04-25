//! Dispatcher settings, runtime mode, task types, and thread-spawner abstraction.
//!
//! Purpose:
//! - Dispatcher settings, runtime mode, task types, and thread-spawner abstraction.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::contracts::WebhookConfig;
use crossbeam_channel::{Receiver, Sender};
use std::fmt;
use std::io;
use std::sync::{Arc, Mutex};

use crate::webhook::types::WebhookMessage;

pub(super) const DEFAULT_QUEUE_CAPACITY: usize = 500;
pub(super) const DEFAULT_WORKER_COUNT: usize = 4;
pub(super) const MAX_QUEUE_CAPACITY: usize = 10_000;
pub(super) const DISPATCHER_STARTUP_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatcherSettings {
    pub(crate) queue_capacity: usize,
    pub(crate) worker_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeMode {
    Standard,
    Parallel { worker_count: u8 },
}

pub(crate) struct WebhookDispatcher {
    pub(crate) settings: DispatcherSettings,
    pub(crate) ready_sender: Arc<Sender<DeliveryTask>>,
    pub(crate) retry_sender: Arc<Sender<ScheduledRetry>>,
    pub(crate) shutdown_sender: Sender<()>,
    pub(crate) worker_handles: Mutex<Vec<Box<dyn ThreadHandle>>>,
    pub(crate) scheduler_handle: Mutex<Option<Box<dyn ThreadHandle>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct DeliveryTask {
    pub(crate) msg: WebhookMessage,
    pub(crate) attempt: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ScheduledRetry {
    pub(crate) ready_at: std::time::Instant,
    pub(crate) task: DeliveryTask,
}

pub(crate) trait ThreadHandle: Send {
    fn join(self: Box<Self>) -> anyhow::Result<()>;
}

pub(crate) trait ThreadSpawner {
    fn spawn(
        &self,
        name: String,
        task: Box<dyn FnOnce() + Send + 'static>,
    ) -> io::Result<Box<dyn ThreadHandle>>;
}

impl ThreadHandle for std::thread::JoinHandle<()> {
    fn join(self: Box<Self>) -> anyhow::Result<()> {
        (*self)
            .join()
            .map_err(|panic_payload| anyhow::anyhow!("thread panicked: {panic_payload:?}"))
    }
}

impl fmt::Debug for WebhookDispatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebhookDispatcher")
            .field("settings", &self.settings)
            .field("ready_sender", &"<sender>")
            .field("retry_sender", &"<sender>")
            .field("shutdown_sender", &"<sender>")
            .field(
                "worker_handles_len",
                &self
                    .worker_handles
                    .lock()
                    .map(|handles| handles.len())
                    .unwrap_or(0),
            )
            .field(
                "has_scheduler_handle",
                &self
                    .scheduler_handle
                    .lock()
                    .map(|handle| handle.is_some())
                    .unwrap_or(false),
            )
            .finish()
    }
}

impl DispatcherSettings {
    pub(super) fn for_mode(config: &WebhookConfig, mode: &RuntimeMode) -> Self {
        let configured_capacity = config
            .queue_capacity
            .unwrap_or(DEFAULT_QUEUE_CAPACITY as u32);
        let base_capacity = configured_capacity as usize;

        match mode {
            RuntimeMode::Standard => Self {
                queue_capacity: base_capacity,
                worker_count: DEFAULT_WORKER_COUNT,
            },
            RuntimeMode::Parallel { worker_count } => {
                let multiplier = config.parallel_queue_multiplier.unwrap_or(2.0);
                let scale = (*worker_count as f64 * multiplier as f64).max(1.0);
                let scaled_capacity = ((base_capacity as f64) * scale)
                    .round()
                    .min(MAX_QUEUE_CAPACITY as f64) as usize;

                Self {
                    queue_capacity: scaled_capacity,
                    worker_count: usize::max(DEFAULT_WORKER_COUNT, *worker_count as usize),
                }
            }
        }
    }
}

pub(super) fn shutdown_requested(shutdown: &Receiver<()>) -> bool {
    matches!(
        shutdown.try_recv(),
        Ok(()) | Err(crossbeam_channel::TryRecvError::Disconnected)
    )
}
