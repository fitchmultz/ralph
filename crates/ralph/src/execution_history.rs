//! Execution history facade for ETA estimation and persistence.
//!
//! Purpose:
//! - Execution history facade for ETA estimation and persistence.
//!
//! Responsibilities:
//! - Re-export execution history data models, storage helpers, and averaging logic.
//! - Keep persistence and weighted-estimation concerns split into focused companions.
//!
//! Not handled here:
//! - Real-time progress tracking.
//! - UI or CLI ETA presentation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Public re-exports preserve the existing `crate::execution_history::*` API surface.
//! - Persistence stays compatible with `.ralph/cache/execution_history.json`.

mod analytics;
mod model;
mod storage;

pub use analytics::{get_phase_averages, weighted_average_duration};
pub use model::{ExecutionEntry, ExecutionHistory};
pub use storage::{load_execution_history, record_execution, save_execution_history};

#[cfg(test)]
pub(crate) use analytics::parse_timestamp_to_secs;
#[cfg(test)]
pub(crate) use storage::prune_old_entries;

#[cfg(test)]
mod tests;
