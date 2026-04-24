//! Asynchronous webhook notification system with bounded queue.
//!
//! Purpose:
//! - Asynchronous webhook notification system with bounded queue.
//!
//! Responsibilities:
//! - Enqueue webhook events to a background worker (non-blocking).
//! - Background worker handles HTTP delivery with retries.
//! - Bounded queue with configurable backpressure policy.
//! - Generate HMAC-SHA256 signatures for webhook verification.
//! - Expose delivery diagnostics and bounded replay controls for failed events.
//!
//! Non-scope:
//! - Webhook endpoint management or registration.
//! - UI mode detection (callers should suppress if desired).
//! - Response processing beyond HTTP status check.
//! - Exactly-once delivery guarantees.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Webhook failures are logged but never fail the calling operation.
//! - Secrets are never logged or exposed in error messages.
//! - All requests include a timeout to prevent hanging.
//! - Queue backpressure protects interactive UX from slow endpoints.
//! - Worker thread is automatically cleaned up on drop.

// Submodules
mod diagnostics;
mod notifications;
mod types;
mod worker;

// Re-export WebhookQueuePolicy from contracts for test compatibility
pub use crate::contracts::WebhookQueuePolicy;

// Public re-exports
pub use diagnostics::{
    ReplayCandidate, ReplayReport, ReplaySelector, WebhookDiagnostics, WebhookFailureRecord,
    diagnostics_snapshot, failure_store_path, replay_failed_deliveries,
};
pub use notifications::{
    notify_loop_started, notify_loop_stopped, notify_phase_completed, notify_phase_started,
    notify_queue_unblocked, notify_status_changed, notify_task_completed,
    notify_task_completed_with_context, notify_task_created, notify_task_created_with_context,
    notify_task_failed, notify_task_failed_with_context, notify_task_started,
    notify_task_started_with_context, send_webhook, send_webhook_payload,
};
pub use types::{ResolvedWebhookConfig, WebhookContext, WebhookEventType, WebhookPayload};
pub use worker::init_worker_for_parallel;

// Internal re-exports for use within the crate
pub(crate) use types::{WebhookMessage, resolve_webhook_config};
pub(crate) use worker::enqueue_webhook_payload_for_replay;

#[cfg(test)]
mod tests;
