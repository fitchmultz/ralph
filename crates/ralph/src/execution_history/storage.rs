//! Execution-history persistence helpers.
//!
//! Purpose:
//! - Execution-history persistence helpers.
//!
//! Responsibilities:
//! - Load and save execution-history cache files.
//! - Append completed executions and prune old entries.
//!
//! Not handled here:
//! - Weighted-average calculations.
//! - ETA presentation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - History persists at `.ralph/cache/execution_history.json`.
//! - Pruning keeps only the newest bounded set of entries.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::progress::ExecutionPhase;

use super::model::{ExecutionEntry, ExecutionHistory};

const EXECUTION_HISTORY_FILE: &str = "execution_history.json";
const MAX_ENTRIES: usize = 100;

fn execution_history_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(EXECUTION_HISTORY_FILE)
}

/// Load execution history from cache directory.
pub fn load_execution_history(cache_dir: &Path) -> Result<ExecutionHistory> {
    let path = execution_history_path(cache_dir);
    if !path.exists() {
        return Ok(ExecutionHistory::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read execution history from {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse execution history from {}", path.display()))
}

/// Save execution history to cache directory.
pub fn save_execution_history(history: &ExecutionHistory, cache_dir: &Path) -> Result<()> {
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("Failed to create cache directory {}", cache_dir.display()))?;

    let path = execution_history_path(cache_dir);
    let content =
        serde_json::to_string_pretty(history).context("Failed to serialize execution history")?;
    crate::fsutil::write_atomic(&path, content.as_bytes())
        .with_context(|| format!("Failed to persist execution history to {}", path.display()))
}

/// Record a completed execution to history.
pub fn record_execution(
    task_id: &str,
    runner: &str,
    model: &str,
    phase_count: u8,
    phase_durations: std::collections::HashMap<ExecutionPhase, Duration>,
    total_duration: Duration,
    cache_dir: &Path,
) -> Result<()> {
    let mut history = load_execution_history(cache_dir)?;
    history.entries.push(ExecutionEntry {
        timestamp: crate::timeutil::now_utc_rfc3339_or_fallback(),
        task_id: task_id.to_string(),
        runner: runner.to_string(),
        model: model.to_string(),
        phase_count,
        phase_durations,
        total_duration,
    });

    prune_old_entries(&mut history);
    save_execution_history(&history, cache_dir)
}

/// Prune oldest entries to keep history bounded.
pub(crate) fn prune_old_entries(history: &mut ExecutionHistory) {
    if history.entries.len() <= MAX_ENTRIES {
        return;
    }

    history
        .entries
        .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    history.entries.truncate(MAX_ENTRIES);
}
