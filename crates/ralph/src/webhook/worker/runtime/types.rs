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
use crossbeam_channel::Sender;
use std::io;
use std::sync::Arc;

use crate::webhook::types::WebhookMessage;

pub(super) const DEFAULT_QUEUE_CAPACITY: usize = 500;
pub(super) const DEFAULT_WORKER_COUNT: usize = 4;
pub(super) const MAX_QUEUE_CAPACITY: usize = 10_000;
pub(super) const MAX_PARALLEL_MULTIPLIER: f64 = 10.0;
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

#[derive(Debug)]
pub(crate) struct WebhookDispatcher {
    pub(crate) settings: DispatcherSettings,
    pub(crate) ready_sender: Arc<Sender<DeliveryTask>>,
    pub(crate) retry_sender: Arc<Sender<ScheduledRetry>>,
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

pub(crate) trait ThreadSpawner {
    fn spawn(&self, name: String, task: Box<dyn FnOnce() + Send + 'static>) -> io::Result<()>;
}

impl DispatcherSettings {
    pub(super) fn for_mode(config: &WebhookConfig, mode: &RuntimeMode) -> Self {
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
