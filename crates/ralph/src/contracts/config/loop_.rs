//! Run loop waiting configuration for daemon/continuous mode.
//!
//! Purpose:
//! - Run loop waiting configuration for daemon/continuous mode.
//!
//! Responsibilities:
//! - Define loop config struct and merge behavior.
//!
//! Not handled here:
//! - Loop execution logic (see `crate::commands::run` module).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Run loop waiting configuration for daemon/continuous mode.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LoopConfig {
    /// Wait when queue is empty instead of exiting (continuous/daemon mode).
    pub wait_when_empty: Option<bool>,

    /// Poll interval in milliseconds while waiting for new tasks when queue is empty.
    /// Default: 30000 (30 seconds). Minimum: 50.
    #[schemars(range(min = 50))]
    pub empty_poll_ms: Option<u64>,

    /// Wait when blocked by dependencies/schedule instead of exiting.
    pub wait_when_blocked: Option<bool>,

    /// Poll interval in milliseconds while waiting for blocked tasks to become runnable.
    /// Default: 1000 (1 second). Minimum: 50.
    #[schemars(range(min = 50))]
    pub wait_poll_ms: Option<u64>,

    /// Timeout in seconds for waiting (0 = no timeout).
    #[schemars(range(min = 0))]
    pub wait_timeout_seconds: Option<u64>,

    /// Send notification when queue transitions from blocked to runnable.
    pub notify_when_unblocked: Option<bool>,
}

impl LoopConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.wait_when_empty.is_some() {
            self.wait_when_empty = other.wait_when_empty;
        }
        if other.empty_poll_ms.is_some() {
            self.empty_poll_ms = other.empty_poll_ms;
        }
        if other.wait_when_blocked.is_some() {
            self.wait_when_blocked = other.wait_when_blocked;
        }
        if other.wait_poll_ms.is_some() {
            self.wait_poll_ms = other.wait_poll_ms;
        }
        if other.wait_timeout_seconds.is_some() {
            self.wait_timeout_seconds = other.wait_timeout_seconds;
        }
        if other.notify_when_unblocked.is_some() {
            self.notify_when_unblocked = other.notify_when_unblocked;
        }
    }
}
