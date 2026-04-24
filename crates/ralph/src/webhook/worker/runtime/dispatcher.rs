//! Webhook dispatcher construction, thread startup, and teardown.
//!
//! Purpose:
//! - Webhook dispatcher construction, thread startup, and teardown.
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

use anyhow::Context;
use crossbeam_channel::{bounded, unbounded};
use std::io;
use std::sync::Arc;

use super::scheduler::retry_scheduler_loop;
use super::types::{
    DISPATCHER_STARTUP_TIMEOUT, DispatcherSettings, ThreadSpawner, WebhookDispatcher,
};
use super::worker_loop::worker_loop;
use crate::webhook::diagnostics;

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

impl WebhookDispatcher {
    pub(super) fn new(settings: DispatcherSettings) -> anyhow::Result<Arc<Self>> {
        Self::new_with_spawner(settings, &OsThreadSpawner)
    }

    pub(super) fn new_with_spawner(
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
