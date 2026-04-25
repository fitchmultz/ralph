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
    fn spawn(
        &self,
        name: String,
        task: Box<dyn FnOnce() + Send + 'static>,
    ) -> io::Result<Box<dyn super::types::ThreadHandle>> {
        std::thread::Builder::new()
            .name(name)
            .spawn(task)
            .map(|handle| Box::new(handle) as Box<dyn super::types::ThreadHandle>)
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
        let (shutdown_sender, shutdown_receiver) = unbounded();
        let startup_signals = settings.worker_count.saturating_add(1);
        let (startup_sender, startup_receiver) = bounded(startup_signals);
        let mut worker_handles = Vec::with_capacity(settings.worker_count);
        let mut scheduler_handle_opt = None;

        let ready_sender = Arc::new(ready_sender);
        let retry_sender = Arc::new(retry_sender);

        for worker_id in 0..settings.worker_count {
            let ready_receiver = ready_receiver.clone();
            let weak_retry_sender = Arc::downgrade(&retry_sender);
            let startup_sender = startup_sender.clone();
            let thread_name = format!("ralph-webhook-worker-{worker_id}");
            let handle = match spawner
                .spawn(
                    thread_name,
                    Box::new(move || {
                        if let Err(err) = startup_sender.send(()) {
                            log::warn!(
                                "Webhook delivery worker startup signal skipped because dispatcher startup was abandoned: {err}"
                            );
                            return;
                        }

                        worker_loop(ready_receiver, weak_retry_sender)
                    }),
                )
                .with_context(|| format!("spawn webhook delivery worker {worker_id}"))
            {
                Ok(handle) => handle,
                Err(err) => {
                    teardown_partial_dispatcher(
                        shutdown_sender,
                        ready_sender,
                        retry_sender,
                        worker_handles,
                        scheduler_handle_opt,
                    );
                    return Err(err);
                }
            };
            worker_handles.push(handle);
        }

        let scheduler_ready = Arc::downgrade(&ready_sender);
        let scheduler_startup_sender = startup_sender.clone();
        let scheduler_shutdown_receiver = shutdown_receiver.clone();
        let scheduler_handle = match spawner
            .spawn(
                "ralph-webhook-retry-scheduler".to_string(),
                Box::new(move || {
                    if let Err(err) = scheduler_startup_sender.send(()) {
                        log::warn!(
                            "Webhook retry scheduler startup signal skipped because dispatcher startup was abandoned: {err}"
                        );
                        return;
                    }

                    retry_scheduler_loop(retry_receiver, scheduler_ready, scheduler_shutdown_receiver)
                }),
            )
            .context("spawn webhook retry scheduler")
        {
            Ok(handle) => handle,
            Err(err) => {
                teardown_partial_dispatcher(
                    shutdown_sender,
                    ready_sender,
                    retry_sender,
                    worker_handles,
                    scheduler_handle_opt,
                );
                return Err(err);
            }
        };
        scheduler_handle_opt = Some(scheduler_handle);
        drop(startup_sender);

        for started_count in 0..startup_signals {
            if let Err(err) = startup_receiver.recv_timeout(DISPATCHER_STARTUP_TIMEOUT) {
                teardown_partial_dispatcher(
                    shutdown_sender,
                    ready_sender,
                    retry_sender,
                    worker_handles,
                    scheduler_handle_opt,
                );
                anyhow::bail!(
                    "wait for webhook dispatcher thread startup ({started_count}/{startup_signals} ready): {err}"
                );
            }
        }

        let dispatcher = Arc::new(Self {
            settings: settings.clone(),
            ready_sender,
            retry_sender,
            shutdown_sender,
            worker_handles: std::sync::Mutex::new(worker_handles),
            scheduler_handle: std::sync::Mutex::new(scheduler_handle_opt),
        });

        diagnostics::set_queue_capacity(settings.queue_capacity);

        log::debug!(
            "Webhook dispatcher started with {} workers and queue capacity {}",
            settings.worker_count,
            settings.queue_capacity
        );

        Ok(dispatcher)
    }
}

fn teardown_partial_dispatcher(
    shutdown_sender: crossbeam_channel::Sender<()>,
    ready_sender: Arc<crossbeam_channel::Sender<super::types::DeliveryTask>>,
    retry_sender: Arc<crossbeam_channel::Sender<super::types::ScheduledRetry>>,
    worker_handles: Vec<Box<dyn super::types::ThreadHandle>>,
    scheduler_handle: Option<Box<dyn super::types::ThreadHandle>>,
) {
    drop(shutdown_sender);
    drop(retry_sender);
    if let Some(handle) = scheduler_handle {
        let _ = handle.join();
    }
    drop(ready_sender);
    for handle in worker_handles {
        let _ = handle.join();
    }
}

impl Drop for WebhookDispatcher {
    fn drop(&mut self) {
        let (replacement_shutdown, _replacement_shutdown_receiver) = unbounded();
        let shutdown_sender = std::mem::replace(&mut self.shutdown_sender, replacement_shutdown);
        drop(shutdown_sender);

        let (replacement_retry, _replacement_retry_receiver) = unbounded();
        let retry_sender = std::mem::replace(&mut self.retry_sender, Arc::new(replacement_retry));
        drop(retry_sender);

        log::debug!(
            "Webhook dispatcher shutting down (workers: {}, capacity: {})",
            self.settings.worker_count,
            self.settings.queue_capacity
        );

        let mut scheduler_handle = match self.scheduler_handle.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(handle) = scheduler_handle.take()
            && let Err(err) = handle.join()
        {
            log::warn!("Webhook dispatcher scheduler shutdown failed: {err:#}");
        }
        drop(scheduler_handle);

        let (replacement_ready, _replacement_ready_receiver) = bounded(1);
        let ready_sender = std::mem::replace(&mut self.ready_sender, Arc::new(replacement_ready));
        drop(ready_sender);

        let mut worker_handles = match self.worker_handles.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        for handle in worker_handles.drain(..) {
            if let Err(err) = handle.join() {
                log::warn!("Webhook dispatcher worker shutdown failed: {err:#}");
            }
        }

        diagnostics::set_queue_capacity(0);
    }
}
