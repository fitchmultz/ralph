//! Integration test hub for webhook delivery behavior.
//!
//! Purpose:
//! - Integration test hub for webhook delivery behavior.
//!
//! Responsibilities:
//! - Group webhook integration coverage by delivery semantics, filtering, payload shape, and configuration guards.
//! - Keep TCP parsing/bootstrap helpers in adjacent suite-local support.
//! - Preserve the shared integration-test support surface through `mod test_support`.
//!
//! Not handled here:
//! - Webhook unit tests under `src/webhook/tests.rs`.
//! - External network behavior beyond local listener simulations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests remain `#[serial]` because webhook delivery uses a process-global worker.
//! - Deterministic waiting goes through shared sync helpers instead of ad hoc sleeps.

mod test_support;

#[path = "webhook_integration_test/webhook_integration_test_configuration.rs"]
mod webhook_integration_test_configuration;
#[path = "webhook_integration_test/webhook_integration_test_delivery.rs"]
mod webhook_integration_test_delivery;
#[path = "webhook_integration_test/webhook_integration_test_filtering.rs"]
mod webhook_integration_test_filtering;
#[path = "webhook_integration_test/webhook_integration_test_payloads.rs"]
mod webhook_integration_test_payloads;
#[path = "webhook_integration_test/webhook_integration_test_support.rs"]
mod webhook_integration_test_support;
