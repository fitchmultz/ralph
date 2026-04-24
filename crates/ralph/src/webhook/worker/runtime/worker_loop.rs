//! Webhook delivery worker thread loop.
//!
//! Purpose:
//! - Webhook delivery worker thread loop.
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

use crossbeam_channel::{Receiver, Sender};
use std::sync::Weak;

use super::types::{DeliveryTask, ScheduledRetry};
use crate::webhook::diagnostics;
use crate::webhook::worker::delivery::handle_delivery_task;

pub(super) fn worker_loop(
    ready_receiver: Receiver<DeliveryTask>,
    retry_sender: Weak<Sender<ScheduledRetry>>,
) {
    while let Ok(task) = ready_receiver.recv() {
        diagnostics::note_queue_dequeue();
        handle_delivery_task(task, &retry_sender);
    }
}
