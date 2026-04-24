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

use crossbeam_channel::{Receiver, Sender, TryRecvError};
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
mod tests {
    use super::*;
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
}
