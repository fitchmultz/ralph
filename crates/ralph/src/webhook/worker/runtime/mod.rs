//! Purpose: Own the reloadable webhook dispatcher runtime and its worker/scheduler lifecycle.
//!
//! Responsibilities:
//! - Build and rebuild dispatcher state from webhook runtime mode and config.
//! - Start delivery workers and the retry scheduler with deterministic startup behavior.
//! - Route ready delivery tasks to delivery helpers without blocking enqueue callers.
//!
//! Scope:
//! - Dispatcher lifecycle, thread startup/teardown, queue sizing, and retry scheduling orchestration.
//!
//! Usage:
//! - Called by webhook enqueue helpers and test-only runtime controls through the worker facade.
//!
//! Invariants/Assumptions:
//! - Runtime settings are rebuilt when the effective mode/config changes.
//! - Retry scheduling stays off worker threads so failing endpoints do not sleep in place.
//! - Dispatcher teardown must not leak background threads or retain stale queue channels across rebuilds.
//! - When the inbound retry channel disconnects during a rebuild, the scheduler still honors pending
//!   `ready_at` deadlines before exiting so in-flight retries are not dropped.

mod dispatcher;
mod scheduler;
mod state;
mod types;
mod worker_loop;

pub use state::init_worker_for_parallel;

pub(crate) use state::dispatcher_for_config;
pub(crate) use types::{DeliveryTask, ScheduledRetry};

#[cfg(test)]
pub(crate) use state::{current_dispatcher_settings_for_tests, reset_dispatcher_for_tests};

#[cfg(test)]
mod tests;
