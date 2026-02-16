//! Queue configuration structs and aging thresholds.
//!
//! Responsibilities:
//! - Define queue-related configuration structs and merge behavior.
//!
//! Not handled here:
//! - Queue file IO (see `crate::queue` module).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Aging threshold configuration for `ralph queue aging`.
///
/// Controls the day thresholds for categorizing tasks by age.
/// Ordering invariant: warning_days < stale_days < rotten_days
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct QueueAgingThresholds {
    /// Warn when task age is strictly greater than this many days.
    #[schemars(range(min = 0, max = 3650))]
    pub warning_days: Option<u32>,

    /// Mark as stale when age is strictly greater than this many days.
    #[schemars(range(min = 0, max = 3650))]
    pub stale_days: Option<u32>,

    /// Mark as rotten when age is strictly greater than this many days.
    #[schemars(range(min = 0, max = 3650))]
    pub rotten_days: Option<u32>,
}

/// Queue-related configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct QueueConfig {
    /// Path to the JSON queue file, relative to repo root.
    ///
    /// Paths are intended to be repo-root relative. Parallel mode requires the
    /// resolved path to be under the repo root (no `..`) so it can be copied
    /// into workspace clones.
    pub file: Option<PathBuf>,

    /// Path to the JSON done archive file, relative to repo root.
    ///
    /// Paths are intended to be repo-root relative. Parallel mode requires the
    /// resolved path to be under the repo root (no `..`) so it can be copied
    /// into workspace clones.
    pub done_file: Option<PathBuf>,

    /// ID prefix (default: "RQ").
    pub id_prefix: Option<String>,

    /// Zero pad width for the numeric suffix (default: 4 -> RQ-0001).
    pub id_width: Option<u8>,

    /// Warning threshold for queue file size in KB (default: 500).
    #[schemars(range(min = 100, max = 10000))]
    pub size_warning_threshold_kb: Option<u32>,

    /// Warning threshold for number of tasks in queue (default: 500).
    #[schemars(range(min = 50, max = 5000))]
    pub task_count_warning_threshold: Option<u32>,

    /// Maximum allowed dependency chain depth before warning (default: 10).
    #[schemars(range(min = 1, max = 100))]
    pub max_dependency_depth: Option<u8>,

    /// Auto-archive terminal tasks (done/rejected) from queue to done after this many days.
    ///
    /// Semantics:
    /// - None: disabled (default)
    /// - Some(0): archive immediately when the sweep runs
    /// - Some(N): archive when completed_at is at least N days old
    ///
    /// The sweep runs after selected queue mutation operations (e.g., task edits and run supervision).
    /// Tasks with missing or invalid completed_at timestamps are not moved when N > 0.
    #[schemars(range(min = 0, max = 3650))]
    pub auto_archive_terminal_after_days: Option<u32>,

    /// Thresholds for `ralph queue aging` buckets.
    ///
    /// Default: warning>7d, stale>14d, rotten>30d.
    /// Ordering must satisfy: warning_days < stale_days < rotten_days.
    pub aging_thresholds: Option<QueueAgingThresholds>,
}

impl QueueConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.file.is_some() {
            self.file = other.file;
        }
        if other.done_file.is_some() {
            self.done_file = other.done_file;
        }
        if other.id_prefix.is_some() {
            self.id_prefix = other.id_prefix;
        }
        if other.id_width.is_some() {
            self.id_width = other.id_width;
        }
        if other.size_warning_threshold_kb.is_some() {
            self.size_warning_threshold_kb = other.size_warning_threshold_kb;
        }
        if other.task_count_warning_threshold.is_some() {
            self.task_count_warning_threshold = other.task_count_warning_threshold;
        }
        if other.max_dependency_depth.is_some() {
            self.max_dependency_depth = other.max_dependency_depth;
        }
        if other.auto_archive_terminal_after_days.is_some() {
            self.auto_archive_terminal_after_days = other.auto_archive_terminal_after_days;
        }
        if other.aging_thresholds.is_some() {
            self.aging_thresholds = other.aging_thresholds;
        }
    }
}
