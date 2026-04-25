//! Retry queue ordering and scheduler thread loop.
//!
//! Purpose:
//! - Retry queue ordering and scheduler thread loop.
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

use crossbeam_channel::{Receiver, Sender, select};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::sync::Weak;
use std::time::Instant;

use super::types::{DeliveryTask, ScheduledRetry};
use crate::webhook::diagnostics;

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

pub(super) fn retry_scheduler_loop(
    retry_receiver: Receiver<ScheduledRetry>,
    ready_sender: Weak<Sender<DeliveryTask>>,
    shutdown_receiver: Receiver<()>,
) {
    let mut pending = BinaryHeap::<RetryQueueEntry>::new();

    loop {
        let timeout = pending
            .peek()
            .map(|entry| entry.0.ready_at.saturating_duration_since(Instant::now()));

        let scheduled = match timeout {
            Some(duration) => {
                let timeout_receiver = crossbeam_channel::after(duration);
                select! {
                    recv(shutdown_receiver) -> _ => {
                        flush_pending_retries(&mut pending, &ready_sender);
                        break;
                    }
                    recv(retry_receiver) -> msg => match msg {
                        Ok(task) => Some(task),
                        Err(_) => {
                            flush_pending_retries(&mut pending, &ready_sender);
                            break;
                        }
                    },
                    recv(timeout_receiver) -> _ => None,
                }
            }
            None => {
                select! {
                    recv(shutdown_receiver) -> _ => {
                        flush_pending_retries(&mut pending, &ready_sender);
                        break;
                    }
                    recv(retry_receiver) -> msg => match msg {
                        Ok(task) => Some(task),
                        Err(_) => {
                            flush_pending_retries(&mut pending, &ready_sender);
                            break;
                        }
                    }
                }
            }
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

        if super::types::shutdown_requested(&shutdown_receiver) {
            flush_pending_retries(&mut pending, &ready_sender);
            break;
        }
    }
}

fn flush_pending_retries(
    pending: &mut BinaryHeap<RetryQueueEntry>,
    ready_sender: &Weak<Sender<DeliveryTask>>,
) {
    while let Some(RetryQueueEntry(scheduled)) = pending.pop() {
        let Some(ready_sender) = ready_sender.upgrade() else {
            let error = anyhow::anyhow!(
                "webhook dispatcher shut down before retry requeue drain: ready queue unavailable"
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
                    "webhook dispatcher shut down before retry requeue drain: {send_err}"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::contracts::WebhookConfig;
    use crate::webhook::types::{WebhookMessage, WebhookPayload, resolve_webhook_config};

    fn minimal_task(attempt: u32) -> DeliveryTask {
        DeliveryTask {
            msg: WebhookMessage {
                payload: WebhookPayload {
                    event: "e".to_string(),
                    timestamp: "t".to_string(),
                    task_id: None,
                    task_title: None,
                    previous_status: None,
                    current_status: None,
                    note: None,
                    context: Default::default(),
                },
                config: resolve_webhook_config(&WebhookConfig::default()),
            },
            attempt,
        }
    }

    #[test]
    fn retry_heap_orders_earliest_ready_first() {
        let now = Instant::now();
        let mut heap = BinaryHeap::<RetryQueueEntry>::new();
        heap.push(RetryQueueEntry(ScheduledRetry {
            ready_at: now + Duration::from_secs(10),
            task: minimal_task(0),
        }));
        heap.push(RetryQueueEntry(ScheduledRetry {
            ready_at: now + Duration::from_secs(1),
            task: minimal_task(1),
        }));
        let first = heap.pop().expect("entry");
        assert_eq!(first.0.task.attempt, 1);
    }

    #[test]
    fn scheduler_drains_pending_retries_when_shutdown_begins() {
        let (ready_sender, ready_receiver) = unbounded::<DeliveryTask>();
        let (retry_sender, retry_receiver) = unbounded::<ScheduledRetry>();
        let (shutdown_sender, shutdown_receiver) = unbounded::<()>();
        let ready_sender = Arc::new(ready_sender);

        let scheduler = std::thread::spawn({
            let ready_sender = Arc::downgrade(&ready_sender);
            move || retry_scheduler_loop(retry_receiver, ready_sender, shutdown_receiver)
        });

        retry_sender
            .send(ScheduledRetry {
                ready_at: Instant::now() + Duration::from_secs(60),
                task: minimal_task(2),
            })
            .expect("schedule retry");

        drop(shutdown_sender);

        let drained = ready_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("pending retry should drain immediately");
        assert_eq!(drained.attempt, 2);

        scheduler.join().expect("scheduler should exit cleanly");
    }
}
