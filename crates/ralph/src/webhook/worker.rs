//! Webhook worker facade.
//!
//! Responsibilities:
//! - Re-export webhook runtime initialization, enqueue, and delivery helpers.
//! - Keep dispatcher orchestration, request delivery, and enqueue policy logic split by concern.
//!
//! Not handled here:
//! - Webhook payload/config types (see `super::types`).
//! - Diagnostics persistence and replay selection (see `super::diagnostics`).
//! - Notification convenience functions (see `super::notifications`).
//!
//! Invariants/assumptions:
//! - Public and crate-visible re-exports preserve the existing worker API surface.
//! - Human-visible destinations continue to flow through the redaction helper only.
//! - Retry scheduling remains owned by the runtime layer, not the hot delivery path.

mod delivery;
mod enqueue;
mod runtime;

pub use runtime::init_worker_for_parallel;

pub(crate) use delivery::redact_webhook_destination;
pub(crate) use enqueue::{enqueue_webhook_payload_for_replay, send_webhook_payload_internal};

#[cfg(test)]
pub(crate) use delivery::{generate_signature, install_test_transport_for_tests};
#[cfg(test)]
pub(crate) use runtime::{current_dispatcher_settings_for_tests, reset_dispatcher_for_tests};
