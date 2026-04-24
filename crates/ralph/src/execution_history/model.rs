//! Execution-history data models.
//!
//! Purpose:
//! - Execution-history data models.
//!
//! Responsibilities:
//! - Define persisted execution-history structs.
//! - Provide schema-version defaulting for new histories.
//!
//! Not handled here:
//! - Disk IO.
//! - Weighted-average calculations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `ExecutionHistory::default()` always uses the current schema version.
//! - Entries remain serializable for cache persistence.

use crate::constants::versions::EXECUTION_HISTORY_VERSION;
use crate::progress::ExecutionPhase;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Root execution history data structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionHistory {
    /// Schema version for migrations.
    pub version: u32,
    /// Historical execution entries.
    pub entries: Vec<ExecutionEntry>,
}

impl Default for ExecutionHistory {
    fn default() -> Self {
        Self {
            version: EXECUTION_HISTORY_VERSION,
            entries: Vec::new(),
        }
    }
}

/// A single execution entry recording phase durations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEntry {
    /// When the execution completed (RFC3339).
    pub timestamp: String,
    /// Task ID that was executed.
    pub task_id: String,
    /// Runner used (e.g., "codex", "claude").
    pub runner: String,
    /// Model used (e.g., "sonnet", "gpt-4").
    pub model: String,
    /// Number of phases configured (1, 2, or 3).
    pub phase_count: u8,
    /// Duration for each completed phase.
    pub phase_durations: HashMap<ExecutionPhase, Duration>,
    /// Total execution duration.
    pub total_duration: Duration,
}
