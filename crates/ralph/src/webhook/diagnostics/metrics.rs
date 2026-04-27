//! Purpose: Track in-process webhook delivery metrics and build operator diagnostics snapshots.
//!
//! Responsibilities:
//! - Own atomic queue/delivery counters for the webhook runtime.
//! - Persist exhausted delivery failures through the failure-store companion.
//! - Build diagnostics snapshots from runtime metrics plus persisted failure history.
//!
//! Scope:
//! - Runtime counters and snapshot assembly only.
//!
//! Usage:
//! - Called by webhook worker runtime/delivery/enqueue code and CLI diagnostics commands.
//!
//! Invariants/Assumptions:
//! - Queue capacity reflects runtime state when available, otherwise a clamped config fallback.
//! - Failed-delivery persistence is best-effort and must not panic the hot path.
//! - Counter mutations remain lock-free via atomics.

use super::super::WebhookMessage;
use super::failure_store::{
    WebhookFailureRecord, failure_store_path, load_failure_records, persist_failed_delivery,
};
use crate::contracts::{WebhookConfig, WebhookQueuePolicy};
use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Debug, Clone, Serialize)]
pub struct WebhookDiagnostics {
    pub queue_depth: usize,
    pub queue_capacity: usize,
    pub queue_policy: WebhookQueuePolicy,
    pub enqueued_total: u64,
    pub delivered_total: u64,
    pub failed_total: u64,
    pub dropped_total: u64,
    pub retry_attempts_total: u64,
    pub failure_store_path: String,
    pub recent_failures: Vec<WebhookFailureRecord>,
}

#[derive(Debug, Default)]
struct WebhookMetrics {
    queue_depth: AtomicUsize,
    queue_capacity: AtomicUsize,
    enqueued_total: AtomicU64,
    delivered_total: AtomicU64,
    failed_total: AtomicU64,
    dropped_total: AtomicU64,
    retry_attempts_total: AtomicU64,
}

static METRICS: OnceLock<WebhookMetrics> = OnceLock::new();

fn metrics() -> &'static WebhookMetrics {
    METRICS.get_or_init(WebhookMetrics::default)
}

pub(crate) fn set_queue_capacity(capacity: usize) {
    metrics().queue_capacity.store(capacity, Ordering::SeqCst);
}

pub(crate) fn note_queue_dequeue() {
    let depth = &metrics().queue_depth;
    let _ = depth.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
        Some(current.saturating_sub(1))
    });
}

pub(crate) fn note_enqueue_success() {
    let state = metrics();
    state.enqueued_total.fetch_add(1, Ordering::SeqCst);
    state.queue_depth.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn note_retry_requeue() {
    metrics().queue_depth.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn note_dropped_message() {
    metrics().dropped_total.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn note_retry_attempt() {
    metrics()
        .retry_attempts_total
        .fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn note_delivery_success() {
    metrics().delivered_total.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn note_delivery_failure(msg: &WebhookMessage, err: &anyhow::Error, attempts: u32) {
    metrics().failed_total.fetch_add(1, Ordering::SeqCst);

    if let Err(write_err) = persist_failed_delivery(msg, err, attempts) {
        log::warn!("Failed to persist webhook failure record: {write_err:#}");
    }
}

pub fn diagnostics_snapshot(
    repo_root: &Path,
    config: &WebhookConfig,
    recent_limit: usize,
) -> Result<WebhookDiagnostics> {
    let path = failure_store_path(repo_root);
    let records = load_failure_records(&path)?;
    let limit = if recent_limit == 0 {
        records.len()
    } else {
        recent_limit
    };
    let recent_failures = records.into_iter().rev().take(limit).collect::<Vec<_>>();

    let state = metrics();
    let queue_capacity = if !config.needs_runtime() {
        0
    } else {
        let configured_capacity = config
            .queue_capacity
            .map(|value| value.clamp(1, 10_000) as usize)
            .unwrap_or(500);
        match state.queue_capacity.load(Ordering::SeqCst) {
            0 => configured_capacity,
            value => value,
        }
    };

    Ok(WebhookDiagnostics {
        queue_depth: state.queue_depth.load(Ordering::SeqCst),
        queue_capacity,
        queue_policy: config.queue_policy.unwrap_or_default(),
        enqueued_total: state.enqueued_total.load(Ordering::SeqCst),
        delivered_total: state.delivered_total.load(Ordering::SeqCst),
        failed_total: state.failed_total.load(Ordering::SeqCst),
        dropped_total: state.dropped_total.load(Ordering::SeqCst),
        retry_attempts_total: state.retry_attempts_total.load(Ordering::SeqCst),
        failure_store_path: path.display().to_string(),
        recent_failures,
    })
}

#[cfg(test)]
pub(super) fn reset_metrics_for_tests() {
    let state = metrics();
    state.queue_depth.store(0, Ordering::SeqCst);
    state.queue_capacity.store(0, Ordering::SeqCst);
    state.enqueued_total.store(0, Ordering::SeqCst);
    state.delivered_total.store(0, Ordering::SeqCst);
    state.failed_total.store(0, Ordering::SeqCst);
    state.dropped_total.store(0, Ordering::SeqCst);
    state.retry_attempts_total.store(0, Ordering::SeqCst);
}
