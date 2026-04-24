//! Desktop notification configuration.
//!
//! Purpose:
//! - Desktop notification configuration.
//!
//! Responsibilities:
//! - Define notification config struct and merge behavior.
//!
//! Not handled here:
//! - Actual notification delivery (see platform-specific notification modules).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Desktop notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct NotificationConfig {
    /// Enable desktop notifications on task completion (default: true).
    /// This is the legacy/compatibility field; prefer `notify_on_complete`.
    pub enabled: Option<bool>,

    /// Enable desktop notifications on task completion (default: true).
    pub notify_on_complete: Option<bool>,

    /// Enable desktop notifications on task failure (default: true).
    pub notify_on_fail: Option<bool>,

    /// Enable desktop notifications when loop mode completes (default: true).
    pub notify_on_loop_complete: Option<bool>,

    /// Enable desktop notifications when watch mode adds new tasks from comments (default: true).
    pub notify_on_watch_new_tasks: Option<bool>,

    /// Suppress notifications when a foreground UI client is active (default: true).
    pub suppress_when_active: Option<bool>,

    /// Enable sound alerts with notifications (default: false).
    pub sound_enabled: Option<bool>,

    /// Custom sound file path (platform-specific format).
    /// If not set, uses platform default sounds.
    pub sound_path: Option<String>,

    /// Notification timeout in milliseconds (default: 8000).
    #[schemars(range(min = 1000, max = 60000))]
    pub timeout_ms: Option<u32>,
}

impl NotificationConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.enabled.is_some() {
            self.enabled = other.enabled;
        }
        if other.notify_on_complete.is_some() {
            self.notify_on_complete = other.notify_on_complete;
        }
        if other.notify_on_fail.is_some() {
            self.notify_on_fail = other.notify_on_fail;
        }
        if other.notify_on_loop_complete.is_some() {
            self.notify_on_loop_complete = other.notify_on_loop_complete;
        }
        if other.notify_on_watch_new_tasks.is_some() {
            self.notify_on_watch_new_tasks = other.notify_on_watch_new_tasks;
        }
        if other.suppress_when_active.is_some() {
            self.suppress_when_active = other.suppress_when_active;
        }
        if other.sound_enabled.is_some() {
            self.sound_enabled = other.sound_enabled;
        }
        if other.sound_path.is_some() {
            self.sound_path = other.sound_path;
        }
        if other.timeout_ms.is_some() {
            self.timeout_ms = other.timeout_ms;
        }
    }
}
