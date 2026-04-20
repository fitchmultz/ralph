//! Webhook delivery worker thread loop.

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
